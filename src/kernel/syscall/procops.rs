use super::sysinfo::TimeVal;
use super::{define_syscall32, register_syscall};
use crate::elf::ParsedElf32;
use crate::io::Buffer;
use crate::kernel::constants::{
    ENOSYS, PR_GET_NAME, PR_SET_NAME, RLIMIT_STACK, SIG_BLOCK, SIG_SETMASK, SIG_UNBLOCK,
};
use crate::kernel::mem::{Page, PageBuffer, VAddr};
use crate::kernel::task::{
    KernelStack, ProcessBuilder, ProcessList, Signal, SignalAction, Thread, ThreadBuilder,
    ThreadRunnable, UserDescriptor, WaitObject, WaitType,
};
use crate::kernel::user::dataflow::UserString;
use crate::kernel::user::{UserPointer, UserPointerMut};
use crate::kernel::vfs::dentry::Dentry;
use crate::kernel::vfs::{self, FsContext};
use crate::path::Path;
use crate::{kernel::user::dataflow::UserBuffer, prelude::*};
use alloc::borrow::ToOwned;
use alloc::ffi::CString;
use arch::{ExtendedContext, InterruptContext};
use bindings::{EINVAL, ENOENT, ENOTDIR, ERANGE, ESRCH};
use bitflags::bitflags;
use eonix_runtime::scheduler::Scheduler;
use eonix_sync::AsProof as _;

fn do_umask(mask: u32) -> KResult<u32> {
    let context = FsContext::get_current();
    let mut umask = context.umask.lock();

    let old = *umask;
    *umask = mask & 0o777;
    Ok(old)
}

fn do_getcwd(buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let context = FsContext::get_current();
    let mut user_buffer = UserBuffer::new(buffer, bufsize)?;

    let page = Page::alloc_one();
    let mut buffer = PageBuffer::new(page.clone());
    context.cwd.lock().get_path(&context, &mut buffer)?;
    user_buffer.fill(page.as_slice())?.ok_or(ERANGE)?;

    Ok(buffer.wrote())
}

fn do_chdir(path: *const u8) -> KResult<()> {
    let context = FsContext::get_current();
    let path = UserString::new(path)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&context, path, true)?;
    if !dentry.is_valid() {
        return Err(ENOENT);
    }

    if !dentry.is_directory() {
        return Err(ENOTDIR);
    }

    *context.cwd.lock() = dentry;
    Ok(())
}

fn do_mount(source: *const u8, target: *const u8, fstype: *const u8, flags: usize) -> KResult<()> {
    let source = UserString::new(source)?;
    let target = UserString::new(target)?;
    let fstype = UserString::new(fstype)?;

    let context = FsContext::get_current();
    let mountpoint = Dentry::open(&context, Path::new(target.as_cstr().to_bytes())?, true)?;
    if !mountpoint.is_valid() {
        return Err(ENOENT);
    }

    vfs::mount::do_mount(
        &mountpoint,
        source.as_cstr().to_str().map_err(|_| EINVAL)?,
        target.as_cstr().to_str().map_err(|_| EINVAL)?,
        fstype.as_cstr().to_str().map_err(|_| EINVAL)?,
        flags as u64,
    )
}

/// # Return
/// `(entry_ip, sp)`
fn do_execve(exec: &[u8], argv: Vec<CString>, envp: Vec<CString>) -> KResult<(VAddr, VAddr)> {
    let dentry = Dentry::open(&FsContext::get_current(), Path::new(exec)?, true)?;
    if !dentry.is_valid() {
        return Err(ENOENT);
    }

    // TODO: When `execve` is called by one of the threads in a process, the other threads
    //       should be terminated and `execve` is performed in the thread group leader.
    let elf = ParsedElf32::parse(dentry.clone())?;
    let result = elf.load(argv, envp);
    if let Ok((ip, sp, mm_list)) = result {
        Thread::current().process.mm_list.replace(mm_list);
        Thread::current().files.on_exec();
        Thread::current().signal_list.clear_non_ignore();
        Thread::current().set_name(dentry.name().clone());

        Ok((ip, sp))
    } else {
        drop(dentry);

        // We can't hold any ownership when we call `kill_current`.
        ProcessList::kill_current(Signal::SIGSEGV);
    }
}

fn sys_execve(int_stack: &mut InterruptContext, _: &mut ExtendedContext) -> usize {
    match (|| -> KResult<()> {
        let exec = int_stack.rbx as *const u8;
        let exec = UserString::new(exec)?;

        // TODO!!!!!: copy from user
        let mut argv = UserPointer::<u32>::new_vaddr(int_stack.rcx as _)?;
        let mut envp = UserPointer::<u32>::new_vaddr(int_stack.rdx as _)?;

        let mut argv_vec = Vec::new();
        let mut envp_vec = Vec::new();

        loop {
            let arg = argv.read()?;
            if arg == 0 {
                break;
            }

            let arg = UserString::new(arg as *const u8)?;
            argv_vec.push(arg.as_cstr().to_owned());
            argv = argv.offset(1)?;
        }

        loop {
            let arg = envp.read()?;
            if arg == 0 {
                break;
            }

            let arg = UserString::new(arg as *const u8)?;
            envp_vec.push(arg.as_cstr().to_owned());
            envp = envp.offset(1)?;
        }

        let (ip, sp) = do_execve(exec.as_cstr().to_bytes(), argv_vec, envp_vec)?;

        int_stack.rip = ip.0 as u64;
        int_stack.rsp = sp.0 as u64;
        Ok(())
    })() {
        Ok(_) => 0,
        Err(err) => -(err as i32) as _,
    }
}

fn sys_exit(int_stack: &mut InterruptContext, _: &mut ExtendedContext) -> usize {
    let status = int_stack.rbx as u32;

    unsafe {
        let mut procs = ProcessList::get().write();
        eonix_preempt::disable();

        // SAFETY: Preemption is disabled.
        procs.do_kill_process(&Thread::current().process, WaitType::Exited(status));
    }

    unsafe {
        // SAFETY: Preempt count == 1.
        Thread::exit();
    }
}

bitflags! {
    pub struct UserWaitOptions: u32 {
        const WNOHANG = 1;
        const WUNTRACED = 2;
        const WCONTINUED = 8;
    }
}

fn do_waitpid(_waitpid: u32, arg1: *mut u32, options: u32) -> KResult<u32> {
    // if waitpid != u32::MAX {
    //     unimplemented!("waitpid with pid {waitpid}")
    // }
    let options = match UserWaitOptions::from_bits(options) {
        None => unimplemented!("waitpid with options {options}"),
        Some(options) => options,
    };

    let wait_object = Thread::current().process.wait(
        options.contains(UserWaitOptions::WNOHANG),
        options.contains(UserWaitOptions::WUNTRACED),
        options.contains(UserWaitOptions::WCONTINUED),
    )?;

    match wait_object {
        None => Ok(0),
        Some(WaitObject { pid, code }) => {
            if !arg1.is_null() {
                UserPointerMut::new(arg1)?.write(code.to_wstatus())?;
            }
            Ok(pid)
        }
    }
}

fn do_wait4(waitpid: u32, arg1: *mut u32, options: u32, rusage: *mut ()) -> KResult<u32> {
    if rusage.is_null() {
        do_waitpid(waitpid, arg1, options)
    } else {
        unimplemented!("wait4 with rusage")
    }
}

fn do_setsid() -> KResult<u32> {
    Thread::current().process.setsid()
}

fn do_setpgid(pid: u32, pgid: i32) -> KResult<()> {
    let pid = if pid == 0 { Thread::current().process.pid } else { pid };

    let pgid = match pgid {
        0 => pid,
        1.. => pgid as u32,
        _ => return Err(EINVAL),
    };

    Thread::current().process.setpgid(pid, pgid)
}

fn do_getsid(pid: u32) -> KResult<u32> {
    if pid == 0 {
        Ok(Thread::current().process.session_rcu().sid)
    } else {
        let procs = ProcessList::get().read();
        procs
            .try_find_process(pid)
            .map(|proc| proc.session(procs.prove()).sid)
            .ok_or(ESRCH)
    }
}

fn do_getpgid(pid: u32) -> KResult<u32> {
    if pid == 0 {
        Ok(Thread::current().process.pgroup_rcu().pgid)
    } else {
        let procs = ProcessList::get().read();
        procs
            .try_find_process(pid)
            .map(|proc| proc.pgroup(procs.prove()).pgid)
            .ok_or(ESRCH)
    }
}

fn do_getpid() -> KResult<u32> {
    Ok(Thread::current().process.pid)
}

fn do_getppid() -> KResult<u32> {
    Ok(Thread::current().process.parent_rcu().map_or(0, |x| x.pid))
}

fn do_getuid() -> KResult<u32> {
    // All users are root for now.
    Ok(0)
}

fn do_geteuid() -> KResult<u32> {
    // All users are root for now.
    Ok(0)
}

fn do_getgid() -> KResult<u32> {
    // All users are root for now.
    Ok(0)
}

fn do_gettid() -> KResult<u32> {
    Ok(Thread::current().tid)
}

fn do_set_thread_area(desc: *mut UserDescriptor) -> KResult<()> {
    let desc_pointer = UserPointerMut::new(desc)?;
    let mut desc = desc_pointer.read()?;

    Thread::current().set_thread_area(&mut desc)?;
    desc_pointer.write(desc)?;

    // SAFETY: Preemption is disabled on calling `load_thread_area32()`.
    unsafe {
        eonix_preempt::disable();
        Thread::current().load_thread_area32();
        eonix_preempt::enable();
    }

    Ok(())
}

fn do_set_tid_address(tidptr: *mut u32) -> KResult<u32> {
    // TODO!!!: Implement this. We don't use it for now.
    let _tidptr = UserPointerMut::new(tidptr)?;
    Ok(Thread::current().tid)
}

fn do_prctl(option: u32, arg2: usize) -> KResult<()> {
    match option {
        PR_SET_NAME => {
            let name = UserPointer::new(arg2 as *mut [u8; 16])?.read()?;
            let len = name.iter().position(|&c| c == 0).unwrap_or(15);
            Thread::current().set_name(name[..len].into());
            Ok(())
        }
        PR_GET_NAME => {
            let name = Thread::current().get_name();
            let len = name.len().min(15);
            let name: [u8; 16] = core::array::from_fn(|i| if i < len { name[i] } else { 0 });
            UserPointerMut::new(arg2 as *mut [u8; 16])?.write(name)?;
            Ok(())
        }
        _ => Err(EINVAL),
    }
}

fn do_kill(pid: i32, sig: u32) -> KResult<()> {
    let procs = ProcessList::get().read();
    match pid {
        // Send signal to every process for which the calling process has
        // permission to send signals.
        -1 => unimplemented!("kill with pid -1"),
        // Send signal to every process in the process group.
        0 => Thread::current()
            .process
            .pgroup(procs.prove())
            .raise(Signal::try_from(sig)?, procs.prove()),
        // Send signal to the process with the specified pid.
        1.. => procs
            .try_find_process(pid as u32)
            .ok_or(ESRCH)?
            .raise(Signal::try_from(sig)?, procs.prove()),
        // Send signal to the process group with the specified pgid equals to `-pid`.
        ..-1 => procs
            .try_find_pgroup((-pid) as u32)
            .ok_or(ESRCH)?
            .raise(Signal::try_from(sig)?, procs.prove()),
    }

    Ok(())
}

fn do_tkill(tid: u32, sig: u32) -> KResult<()> {
    ProcessList::get()
        .read()
        .try_find_thread(tid)
        .ok_or(ESRCH)?
        .raise(Signal::try_from(sig)?);
    Ok(())
}

fn do_rt_sigprocmask(how: u32, set: *mut u64, oldset: *mut u64, sigsetsize: usize) -> KResult<()> {
    if sigsetsize != size_of::<u64>() {
        return Err(EINVAL);
    }

    let old_mask = Thread::current().signal_list.get_mask();
    if !oldset.is_null() {
        UserPointerMut::new(oldset)?.write(old_mask)?;
    }

    let new_mask = if !set.is_null() {
        UserPointer::new(set)?.read()?
    } else {
        return Ok(());
    };

    match how {
        SIG_BLOCK => Thread::current().signal_list.mask(new_mask),
        SIG_UNBLOCK => Thread::current().signal_list.unmask(new_mask),
        SIG_SETMASK => Thread::current().signal_list.set_mask(new_mask),
        _ => return Err(EINVAL),
    }

    Ok(())
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct UserSignalAction {
    sa_handler: u32,
    sa_flags: u32,
    sa_restorer: u32,
    sa_mask: u64,
}

impl From<UserSignalAction> for SignalAction {
    fn from(from: UserSignalAction) -> SignalAction {
        SignalAction {
            sa_handler: from.sa_handler as usize,
            sa_flags: from.sa_flags as usize,
            sa_mask: from.sa_mask as usize,
            sa_restorer: from.sa_restorer as usize,
        }
    }
}

impl From<SignalAction> for UserSignalAction {
    fn from(from: SignalAction) -> UserSignalAction {
        UserSignalAction {
            sa_handler: from.sa_handler as u32,
            sa_flags: from.sa_flags as u32,
            sa_mask: from.sa_mask as u64,
            sa_restorer: from.sa_restorer as u32,
        }
    }
}

fn do_rt_sigaction(
    signum: u32,
    act: *const UserSignalAction,
    oldact: *mut UserSignalAction,
    sigsetsize: usize,
) -> KResult<()> {
    let signal = Signal::try_from(signum)?;
    if sigsetsize != size_of::<u64>() || signal.is_now() {
        return Err(EINVAL);
    }

    let old_action = Thread::current().signal_list.get_handler(signal);
    if !oldact.is_null() {
        UserPointerMut::new(oldact)?.write(old_action.into())?;
    }

    if !act.is_null() {
        let new_action = UserPointer::new(act)?.read()?;
        Thread::current()
            .signal_list
            .set_handler(signal, &new_action.into())?;
    }

    Ok(())
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct RLimit {
    rlim_cur: u64,
    rlim_max: u64,
}

fn do_prlimit64(
    pid: u32,
    resource: u32,
    new_limit: *const RLimit,
    old_limit: *mut RLimit,
) -> KResult<()> {
    if pid != 0 {
        return Err(ENOSYS);
    }

    match resource {
        RLIMIT_STACK => {
            if !old_limit.is_null() {
                let old_limit = UserPointerMut::new(old_limit)?;
                let rlimit = RLimit {
                    rlim_cur: 8 * 1024 * 1024,
                    rlim_max: 8 * 1024 * 1024,
                };
                old_limit.write(rlimit)?;
            }

            if !new_limit.is_null() {
                return Err(ENOSYS);
            }
            Ok(())
        }
        _ => Err(ENOSYS),
    }
}

fn do_getrlimit(resource: u32, rlimit: *mut RLimit) -> KResult<()> {
    do_prlimit64(0, resource, core::ptr::null(), rlimit)
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RUsage {
    ru_utime: TimeVal,
    ru_stime: TimeVal,
    ru_maxrss: u32,
    ru_ixrss: u32,
    ru_idrss: u32,
    ru_isrss: u32,
    ru_minflt: u32,
    ru_majflt: u32,
    ru_nswap: u32,
    ru_inblock: u32,
    ru_oublock: u32,
    ru_msgsnd: u32,
    ru_msgrcv: u32,
    ru_nsignals: u32,
    ru_nvcsw: u32,
    ru_nivcsw: u32,
}

fn do_getrusage(who: u32, rusage: *mut RUsage) -> KResult<()> {
    if who != 0 {
        return Err(ENOSYS);
    }

    let rusage = UserPointerMut::new(rusage)?;
    rusage.write(RUsage {
        ru_utime: TimeVal::default(),
        ru_stime: TimeVal::default(),
        ru_maxrss: 0,
        ru_ixrss: 0,
        ru_idrss: 0,
        ru_isrss: 0,
        ru_minflt: 0,
        ru_majflt: 0,
        ru_nswap: 0,
        ru_inblock: 0,
        ru_oublock: 0,
        ru_msgsnd: 0,
        ru_msgrcv: 0,
        ru_nsignals: 0,
        ru_nvcsw: 0,
        ru_nivcsw: 0,
    })?;

    Ok(())
}

fn do_chmod(pathname: *const u8, mode: u32) -> KResult<()> {
    let context = FsContext::get_current();
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&context, path, true)?;
    if !dentry.is_valid() {
        return Err(ENOENT);
    }

    dentry.chmod(mode)
}

define_syscall32!(sys_chdir, do_chdir, path: *const u8);
define_syscall32!(sys_umask, do_umask, mask: u32);
define_syscall32!(sys_getcwd, do_getcwd, buffer: *mut u8, bufsize: usize);
define_syscall32!(sys_waitpid, do_waitpid, waitpid: u32, arg1: *mut u32, options: u32);
define_syscall32!(sys_wait4, do_wait4, waitpid: u32, arg1: *mut u32, options: u32, rusage: *mut ());
define_syscall32!(sys_setsid, do_setsid);
define_syscall32!(sys_setpgid, do_setpgid, pid: u32, pgid: i32);
define_syscall32!(sys_getsid, do_getsid, pid: u32);
define_syscall32!(sys_getpgid, do_getpgid, pid: u32);
define_syscall32!(sys_getpid, do_getpid);
define_syscall32!(sys_getppid, do_getppid);
define_syscall32!(sys_getuid, do_getuid);
define_syscall32!(sys_geteuid, do_geteuid);
define_syscall32!(sys_getgid, do_getgid);
define_syscall32!(sys_gettid, do_gettid);
define_syscall32!(sys_mount, do_mount,
    source: *const u8, target: *const u8,fstype: *const u8, flags: usize);
define_syscall32!(sys_set_thread_area, do_set_thread_area, desc: *mut UserDescriptor);
define_syscall32!(sys_set_tid_address, do_set_tid_address, tidptr: *mut u32);
define_syscall32!(sys_prctl, do_prctl, option: u32, arg2: usize);
define_syscall32!(sys_arch_prctl, do_prctl, option: u32, arg2: usize);
define_syscall32!(sys_kill, do_kill, pid: i32, sig: u32);
define_syscall32!(sys_tkill, do_tkill, tid: u32, sig: u32);
define_syscall32!(sys_rt_sigprocmask, do_rt_sigprocmask,
    how: u32, set: *mut u64, oldset: *mut u64, sigsetsize: usize);
define_syscall32!(sys_rt_sigaction, do_rt_sigaction,
    signum: u32, act: *const UserSignalAction, oldact: *mut UserSignalAction, sigsetsize: usize);
define_syscall32!(sys_prlimit64, do_prlimit64,
    pid: u32, resource: u32, new_limit: *const RLimit, old_limit: *mut RLimit);
define_syscall32!(sys_getrlimit, do_getrlimit, resource: u32, rlimit: *mut RLimit);
define_syscall32!(sys_getrusage, do_getrusage, who: u32, rlimit: *mut RUsage);
define_syscall32!(sys_chmod, do_chmod, pathname: *const u8, mode: u32);

fn sys_vfork(int_stack: &mut InterruptContext, ext: &mut ExtendedContext) -> usize {
    sys_fork(int_stack, ext)
}

fn sys_fork(int_stack: &mut InterruptContext, _: &mut ExtendedContext) -> usize {
    let mut procs = ProcessList::get().write();

    let current = Thread::current();
    let current_process = current.process.clone();
    let current_pgroup = current_process.pgroup(procs.prove()).clone();
    let current_session = current_process.session(procs.prove()).clone();

    let mut new_int_context = int_stack.clone();
    new_int_context.set_return_value(0);

    let thread_builder = ThreadBuilder::new().fork_from(&current);
    let (new_thread, new_process) = ProcessBuilder::new()
        .mm_list(current_process.mm_list.new_cloned())
        .parent(current_process)
        .pgroup(current_pgroup)
        .session(current_session)
        .thread_builder(thread_builder)
        .build(&mut procs);

    Scheduler::get()
        .spawn::<KernelStack, _>(ThreadRunnable::from_context(new_thread, new_int_context));

    new_process.pid as usize
}

fn sys_sigreturn(int_stack: &mut InterruptContext, ext_ctx: &mut ExtendedContext) -> usize {
    let result = Thread::current().signal_list.restore(int_stack, ext_ctx);
    match result {
        Ok(ret) => ret,
        Err(_) => {
            println_warn!("`sigreturn` failed in thread {}!", Thread::current().tid);
            Thread::current().raise(Signal::SIGSEGV);
            0
        }
    }
}

pub(super) fn register() {
    register_syscall!(0x01, exit);
    register_syscall!(0x02, fork);
    register_syscall!(0x07, waitpid);
    register_syscall!(0x0b, execve);
    register_syscall!(0x0c, chdir);
    register_syscall!(0x0f, chmod);
    register_syscall!(0x14, getpid);
    register_syscall!(0x15, mount);
    register_syscall!(0x25, kill);
    register_syscall!(0x2f, getgid);
    register_syscall!(0x39, setpgid);
    register_syscall!(0x3c, umask);
    register_syscall!(0x40, getppid);
    register_syscall!(0x42, setsid);
    register_syscall!(0x4d, getrusage);
    register_syscall!(0x72, wait4);
    register_syscall!(0x77, sigreturn);
    register_syscall!(0x84, getpgid);
    register_syscall!(0x93, getsid);
    register_syscall!(0xac, prctl);
    register_syscall!(0xae, rt_sigaction);
    register_syscall!(0xaf, rt_sigprocmask);
    register_syscall!(0xb7, getcwd);
    register_syscall!(0xbe, vfork);
    register_syscall!(0xbf, getrlimit);
    register_syscall!(0xc7, getuid);
    register_syscall!(0xc8, getgid);
    register_syscall!(0xc9, geteuid);
    register_syscall!(0xca, geteuid);
    register_syscall!(0xe0, gettid);
    register_syscall!(0xee, tkill);
    register_syscall!(0xf3, set_thread_area);
    register_syscall!(0xfc, exit);
    register_syscall!(0x102, set_tid_address);
    register_syscall!(0x154, prlimit64);
    register_syscall!(0x180, arch_prctl);
}
