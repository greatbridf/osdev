use super::sysinfo::TimeVal;
use super::SyscallNoReturn;
use crate::io::Buffer;
use crate::kernel::constants::{EINVAL, ENOENT, ENOTDIR, ERANGE, ESRCH};
use crate::kernel::constants::{
    ENOSYS, PR_GET_NAME, PR_SET_NAME, RLIMIT_STACK, SIG_BLOCK, SIG_SETMASK, SIG_UNBLOCK,
};
use crate::kernel::mem::PageBuffer;
use crate::kernel::task::{
    do_clone, futex_wait, futex_wake, FutexFlags, FutexOp, ProcessList, ProgramLoader, Signal,
    SignalAction, SignalMask, WaitObject, WaitType,
};
use crate::kernel::task::{parse_futexop, CloneArgs};
use crate::kernel::user::dataflow::UserString;
use crate::kernel::user::{UserPointer, UserPointerMut};
use crate::kernel::vfs::{self, dentry::Dentry};
use crate::path::Path;
use crate::SIGNAL_NOW;
use crate::{kernel::user::dataflow::UserBuffer, prelude::*};
use alloc::borrow::ToOwned;
use alloc::ffi::CString;
use bitflags::bitflags;
use eonix_hal::traits::trap::RawTrapContext;
use eonix_mm::address::Addr as _;
use eonix_runtime::task::Task;
use eonix_sync::AsProof as _;
use posix_types::signal::SigAction;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct RLimit {
    rlim_cur: u64,
    rlim_max: u64,
}

bitflags! {
    pub struct UserWaitOptions: u32 {
        const WNOHANG = 1;
        const WUNTRACED = 2;
        const WCONTINUED = 8;
    }
}

#[eonix_macros::define_syscall(0x3c)]
fn umask(mask: u32) -> KResult<u32> {
    let mut umask = thread.fs_context.umask.lock();

    let old = *umask;
    *umask = mask & 0o777;
    Ok(old)
}

#[eonix_macros::define_syscall(0xb7)]
fn getcwd(buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let mut user_buffer = UserBuffer::new(buffer, bufsize)?;
    let mut buffer = PageBuffer::new();

    thread
        .fs_context
        .cwd
        .lock()
        .get_path(&thread.fs_context, &mut buffer)?;

    user_buffer.fill(buffer.data())?.ok_or(ERANGE)?;

    Ok(buffer.wrote())
}

#[eonix_macros::define_syscall(0x0c)]
fn chdir(path: *const u8) -> KResult<()> {
    let path = UserString::new(path)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&thread.fs_context, path, true)?;
    if !dentry.is_valid() {
        return Err(ENOENT);
    }

    if !dentry.is_directory() {
        return Err(ENOTDIR);
    }

    *thread.fs_context.cwd.lock() = dentry;
    Ok(())
}

#[eonix_macros::define_syscall(0x15)]
fn mount(source: *const u8, target: *const u8, fstype: *const u8, flags: usize) -> KResult<()> {
    let source = UserString::new(source)?;
    let target = UserString::new(target)?;
    let fstype = UserString::new(fstype)?;

    let mountpoint = Dentry::open(
        &thread.fs_context,
        Path::new(target.as_cstr().to_bytes())?,
        true,
    )?;

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

fn get_strings(mut ptr_strings: UserPointer<'_, u32>) -> KResult<Vec<CString>> {
    let mut strings = Vec::new();

    loop {
        let addr = ptr_strings.read()?;
        if addr == 0 {
            break;
        }

        let user_string = UserString::new(addr as *const u8)?;
        strings.push(user_string.as_cstr().to_owned());
        ptr_strings = ptr_strings.offset(1)?;
    }

    Ok(strings)
}

#[eonix_macros::define_syscall(0x0b)]
fn execve(exec: *const u8, argv: *const u32, envp: *const u32) -> KResult<()> {
    let exec = UserString::new(exec)?;
    let argv = get_strings(UserPointer::new(argv)?)?;
    let envp = get_strings(UserPointer::new(envp)?)?;

    let dentry = Dentry::open(
        &thread.fs_context,
        Path::new(exec.as_cstr().to_bytes())?,
        true,
    )?;

    if !dentry.is_valid() {
        Err(ENOENT)?;
    }

    // TODO: When `execve` is called by one of the threads in a process, the other threads
    //       should be terminated and `execve` is performed in the thread group leader.
    if let Ok(load_info) = ProgramLoader::parse(dentry.clone())?.load(argv, envp) {
        unsafe {
            // SAFETY: We are doing execve, all other threads are terminated.
            thread.process.mm_list.replace(Some(load_info.mm_list));
        }
        thread.files.on_exec();
        thread.signal_list.clear_non_ignore();
        thread.set_name(dentry.name().clone());

        let mut trap_ctx = thread.trap_ctx.borrow();
        trap_ctx.set_program_counter(load_info.entry_ip.addr());
        trap_ctx.set_stack_pointer(load_info.sp.addr());
        Ok(())
    } else {
        // We can't hold any ownership when we call `kill_current`.
        // ProcessList::kill_current(Signal::SIGSEGV);
        todo!()
    }
}

#[eonix_macros::define_syscall(0x01)]
fn exit(status: u32) -> SyscallNoReturn {
    unsafe {
        let mut procs = Task::block_on(ProcessList::get().write());
        Task::block_on(procs.do_exit(&thread, WaitType::Exited(status), false));
    }

    SyscallNoReturn
}

#[eonix_macros::define_syscall(0xfc)]
fn exit_group(status: u32) -> SyscallNoReturn {
    unsafe {
        let mut procs = Task::block_on(ProcessList::get().write());
        Task::block_on(procs.do_exit(&thread, WaitType::Exited(status), true));
    }

    SyscallNoReturn
}

#[eonix_macros::define_syscall(0x07)]
fn waitpid(_waitpid: u32, arg1: *mut u32, options: u32) -> KResult<u32> {
    // if waitpid != u32::MAX {
    //     unimplemented!("waitpid with pid {waitpid}")
    // }
    let options = match UserWaitOptions::from_bits(options) {
        None => unimplemented!("waitpid with options {options}"),
        Some(options) => options,
    };

    let wait_object = Task::block_on(thread.process.wait(
        options.contains(UserWaitOptions::WNOHANG),
        options.contains(UserWaitOptions::WUNTRACED),
        options.contains(UserWaitOptions::WCONTINUED),
    ))?;

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

#[eonix_macros::define_syscall(0x72)]
fn wait4(waitpid: u32, arg1: *mut u32, options: u32, rusage: *mut ()) -> KResult<u32> {
    if rusage.is_null() {
        sys_waitpid(thread, waitpid, arg1, options)
    } else {
        unimplemented!("wait4 with rusage")
    }
}

#[eonix_macros::define_syscall(0x42)]
fn setsid() -> KResult<u32> {
    thread.process.setsid()
}

#[eonix_macros::define_syscall(0x39)]
fn setpgid(pid: u32, pgid: i32) -> KResult<()> {
    let pid = if pid == 0 { thread.process.pid } else { pid };

    let pgid = match pgid {
        0 => pid,
        1.. => pgid as u32,
        _ => return Err(EINVAL),
    };

    thread.process.setpgid(pid, pgid)
}

#[eonix_macros::define_syscall(0x93)]
fn getsid(pid: u32) -> KResult<u32> {
    if pid == 0 {
        Ok(thread.process.session_rcu().sid)
    } else {
        let procs = Task::block_on(ProcessList::get().read());
        procs
            .try_find_process(pid)
            .map(|proc| proc.session(procs.prove()).sid)
            .ok_or(ESRCH)
    }
}

#[eonix_macros::define_syscall(0x84)]
fn getpgid(pid: u32) -> KResult<u32> {
    if pid == 0 {
        Ok(thread.process.pgroup_rcu().pgid)
    } else {
        let procs = Task::block_on(ProcessList::get().read());
        procs
            .try_find_process(pid)
            .map(|proc| proc.pgroup(procs.prove()).pgid)
            .ok_or(ESRCH)
    }
}

#[eonix_macros::define_syscall(0x14)]
fn getpid() -> KResult<u32> {
    Ok(thread.process.pid)
}

#[eonix_macros::define_syscall(0x40)]
fn getppid() -> KResult<u32> {
    Ok(thread.process.parent_rcu().map_or(0, |x| x.pid))
}

#[eonix_macros::define_syscall(0xc7)]
fn getuid() -> KResult<u32> {
    // All users are root for now.
    Ok(0)
}

#[eonix_macros::define_syscall(0xc9)]
fn geteuid() -> KResult<u32> {
    // All users are root for now.
    Ok(0)
}

#[eonix_macros::define_syscall(0x2f)]
fn getgid() -> KResult<u32> {
    // All users are root for now.
    Ok(0)
}

#[eonix_macros::define_syscall(0xc8)]
fn getgid32() -> KResult<u32> {
    sys_getgid(thread)
}

#[eonix_macros::define_syscall(0xe0)]
fn gettid() -> KResult<u32> {
    Ok(thread.tid)
}

#[eonix_macros::define_syscall(0xf3)]
fn set_thread_area(arch_tls: usize) -> KResult<()> {
    thread.set_user_tls(arch_tls)?;

    // SAFETY: Preemption is disabled on calling `load_thread_area32()`.
    unsafe {
        eonix_preempt::disable();
        thread.load_thread_area32();
        eonix_preempt::enable();
    }

    Ok(())
}

#[eonix_macros::define_syscall(0x102)]
fn set_tid_address(tidptr: usize) -> KResult<u32> {
    Ok(thread.tid)
}

#[eonix_macros::define_syscall(0xac)]
fn prctl(option: u32, arg2: usize) -> KResult<()> {
    match option {
        PR_SET_NAME => {
            let name = UserPointer::new(arg2 as *mut [u8; 16])?.read()?;
            let len = name.iter().position(|&c| c == 0).unwrap_or(15);
            thread.set_name(name[..len].into());
            Ok(())
        }
        PR_GET_NAME => {
            let name = thread.get_name();
            let len = name.len().min(15);
            let name: [u8; 16] = core::array::from_fn(|i| if i < len { name[i] } else { 0 });
            UserPointerMut::new(arg2 as *mut [u8; 16])?.write(name)?;
            Ok(())
        }
        _ => Err(EINVAL),
    }
}

#[eonix_macros::define_syscall(0x25)]
fn kill(pid: i32, sig: u32) -> KResult<()> {
    let procs = Task::block_on(ProcessList::get().read());
    match pid {
        // Send signal to every process for which the calling process has
        // permission to send signals.
        -1 => unimplemented!("kill with pid -1"),
        // Send signal to every process in the process group.
        0 => thread
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

#[eonix_macros::define_syscall(0xee)]
fn tkill(tid: u32, sig: u32) -> KResult<()> {
    Task::block_on(ProcessList::get().read())
        .try_find_thread(tid)
        .ok_or(ESRCH)?
        .raise(Signal::try_from(sig)?);
    Ok(())
}

#[eonix_macros::define_syscall(0xaf)]
fn rt_sigprocmask(how: u32, set: *mut u64, oldset: *mut u64, sigsetsize: usize) -> KResult<()> {
    if sigsetsize != size_of::<u64>() {
        return Err(EINVAL);
    }

    let old_mask = u64::from(thread.signal_list.get_mask());
    if !oldset.is_null() {
        UserPointerMut::new(oldset)?.write(old_mask)?;
    }

    let new_mask = if !set.is_null() {
        SignalMask::from(UserPointer::new(set)?.read()?)
    } else {
        return Ok(());
    };

    match how {
        SIG_BLOCK => thread.signal_list.mask(new_mask),
        SIG_UNBLOCK => thread.signal_list.unmask(new_mask),
        SIG_SETMASK => thread.signal_list.set_mask(new_mask),
        _ => return Err(EINVAL),
    }

    Ok(())
}

#[eonix_macros::define_syscall(0xae)]
fn rt_sigaction(
    signum: u32,
    act: *const SigAction,
    oldact: *mut SigAction,
    sigsetsize: usize,
) -> KResult<()> {
    let signal = Signal::try_from(signum)?;
    if sigsetsize != size_of::<u64>() {
        return Err(EINVAL);
    }

    // SIGKILL and SIGSTOP MUST not be set for a handler.
    if matches!(signal, SIGNAL_NOW!()) {
        return Err(EINVAL);
    }

    let old_action = thread.signal_list.get_action(signal);
    if !oldact.is_null() {
        UserPointerMut::new(oldact)?.write(old_action.into())?;
    }

    if !act.is_null() {
        let new_action = UserPointer::new(act)?.read()?;
        let action: SignalAction = new_action.try_into()?;

        thread.signal_list.set_action(signal, action)?;
    }

    Ok(())
}

#[eonix_macros::define_syscall(0x154)]
fn prlimit64(
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

#[eonix_macros::define_syscall(0xbf)]
fn getrlimit(resource: u32, rlimit: *mut RLimit) -> KResult<()> {
    sys_prlimit64(thread, 0, resource, core::ptr::null(), rlimit)
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

#[eonix_macros::define_syscall(0x4d)]
fn getrusage(who: u32, rusage: *mut RUsage) -> KResult<()> {
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

#[eonix_macros::define_syscall(0x0f)]
fn chmod(pathname: *const u8, mode: u32) -> KResult<()> {
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&thread.fs_context, path, true)?;
    if !dentry.is_valid() {
        return Err(ENOENT);
    }

    dentry.chmod(mode)
}

#[eonix_macros::define_syscall(0xbe)]
fn vfork() -> KResult<u32> {
    let clone_args = CloneArgs::for_vfork();

    do_clone(thread, clone_args)
}

#[eonix_macros::define_syscall(0x02)]
fn fork() -> KResult<u32> {
    let clone_args = CloneArgs::for_fork();

    do_clone(thread, clone_args)
}

// x86-32 riscv
#[eonix_macros::define_syscall(0x78)]
fn clone(
    clone_flags: usize,
    new_sp: usize,
    parent_tidptr: usize,
    tls: usize,
    child_tidptr: usize,
) -> KResult<u32> {
    let clone_args = CloneArgs::for_clone(clone_flags, new_sp, child_tidptr, parent_tidptr, tls)?;

    do_clone(thread, clone_args)
}

#[eonix_macros::define_syscall(0xf0)]
fn futex(
    uaddr: usize,
    op: u32,
    val: u32,
    time_out: usize,
    uaddr2: usize,
    val3: u32,
) -> KResult<usize> {
    let (futex_op, futex_flag) = parse_futexop(op)?;

    let pid = if futex_flag.contains(FutexFlags::FUTEX_PRIVATE) {
        Some(thread.process.pid)
    } else {
        None
    };

    match futex_op {
        FutexOp::FUTEX_WAIT => {
            Task::block_on(futex_wait(uaddr, pid, val as u32, None))?;
            return Ok(0);
        }
        FutexOp::FUTEX_WAKE => {
            return Task::block_on(futex_wake(uaddr, pid, val as u32));
        }
        FutexOp::FUTEX_REQUEUE => {
            todo!()
        }
        _ => {
            todo!()
        }
    }
}

#[eonix_macros::define_syscall(0x77)]
fn sigreturn() -> KResult<SyscallNoReturn> {
    thread
        .signal_list
        .restore(
            &mut thread.trap_ctx.borrow(),
            &mut thread.fpu_state.borrow(),
        )
        .inspect_err(|err| {
            println_warn!(
                "`sigreturn` failed in thread {} with error {err}!",
                thread.tid
            );
            Task::block_on(thread.force_kill(Signal::SIGSEGV));
        })?;

    Ok(SyscallNoReturn)
}

// TODO: This should be for x86 only.
#[eonix_macros::define_syscall(0x180)]
fn arch_prctl(option: u32, addr: u32) -> KResult<u32> {
    sys_arch_prctl(thread, option, addr)
}

pub fn keep_alive() {}
