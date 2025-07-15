use super::SyscallNoReturn;
use crate::io::Buffer;
use crate::kernel::constants::{EINVAL, ENOENT, ENOTDIR, ERANGE, ESRCH};
use crate::kernel::constants::{
    ENOSYS, PR_GET_NAME, PR_SET_NAME, RLIMIT_STACK, SIG_BLOCK, SIG_SETMASK, SIG_UNBLOCK,
};
use crate::kernel::mem::PageBuffer;
use crate::kernel::task::{
    do_clone, futex_wait, futex_wake, FutexFlags, FutexOp, ProcessList, ProgramLoader,
    RobustListHead, SignalAction, Thread, WaitType,
};
use crate::kernel::task::{parse_futexop, CloneArgs};
use crate::kernel::timer::sleep;
use crate::kernel::user::dataflow::{CheckedUserPointer, UserString};
use crate::kernel::user::{UserPointer, UserPointerMut};
use crate::kernel::vfs::{self, dentry::Dentry};
use crate::path::Path;
use crate::{kernel::user::dataflow::UserBuffer, prelude::*};
use alloc::borrow::ToOwned;
use alloc::ffi::CString;
use bitflags::bitflags;
use core::ptr::NonNull;
use core::time::Duration;
use eonix_hal::processor::UserTLS;
use eonix_hal::traits::trap::RawTrapContext;
use eonix_mm::address::{Addr as _, VAddr};
use eonix_runtime::task::Task;
use eonix_sync::AsProof as _;
use posix_types::constants::{P_ALL, P_PID};
use posix_types::ctypes::PtrT;
use posix_types::signal::{SigAction, SigInfo, SigSet, Signal};
use posix_types::stat::TimeVal;
use posix_types::{syscall_no::*, SIGNAL_NOW};

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

#[eonix_macros::define_syscall(SYS_NANOSLEEP)]
fn nanosleep(req: *const (u32, u32), rem: *mut (u32, u32)) -> KResult<usize> {
    let req = UserPointer::new(req)?.read()?;
    let rem = if rem.is_null() {
        None
    } else {
        Some(UserPointerMut::new(rem)?)
    };

    let duration = Duration::from_secs(req.0 as u64) + Duration::from_nanos(req.1 as u64);
    Task::block_on(sleep(duration));

    if let Some(rem) = rem {
        rem.write((0, 0))?;
    }

    Ok(0)
}

#[eonix_macros::define_syscall(SYS_UMASK)]
fn umask(mask: u32) -> KResult<u32> {
    let mut umask = thread.fs_context.umask.lock();

    let old = *umask;
    *umask = mask & 0o777;
    Ok(old)
}

#[eonix_macros::define_syscall(SYS_GETCWD)]
fn getcwd(buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let mut user_buffer = UserBuffer::new(buffer, bufsize)?;
    let mut buffer = PageBuffer::new();

    let cwd = thread.fs_context.cwd.lock().clone();
    cwd.get_path(&thread.fs_context, &mut buffer)?;

    user_buffer.fill(buffer.data())?.ok_or(ERANGE)?;

    Ok(buffer.wrote())
}

#[eonix_macros::define_syscall(SYS_CHDIR)]
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

#[eonix_macros::define_syscall(SYS_MOUNT)]
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

fn get_strings(mut ptr_strings: UserPointer<'_, PtrT>) -> KResult<Vec<CString>> {
    let mut strings = Vec::new();

    loop {
        let ptr = ptr_strings.read()?;
        if ptr.is_null() {
            break;
        }

        let user_string = UserString::new(ptr.addr() as *const u8)?;
        strings.push(user_string.as_cstr().to_owned());
        ptr_strings = ptr_strings.offset(1)?;
    }

    Ok(strings)
}

#[eonix_macros::define_syscall(SYS_EXECVE)]
fn execve(exec: *const u8, argv: *const PtrT, envp: *const PtrT) -> KResult<SyscallNoReturn> {
    let exec = UserString::new(exec)?;
    let exec = exec.as_cstr().to_owned();

    let argv = get_strings(UserPointer::new(argv)?)?;
    let envp = get_strings(UserPointer::new(envp)?)?;

    let dentry = Dentry::open(&thread.fs_context, Path::new(exec.as_bytes())?, true)?;
    if !dentry.is_valid() {
        Err(ENOENT)?;
    }

    // TODO: When `execve` is called by one of the threads in a process, the other threads
    //       should be terminated and `execve` is performed in the thread group leader.
    let load_info =
        ProgramLoader::parse(&thread.fs_context, exec, dentry.clone(), argv, envp)?.load()?;

    if let Some(robust_list) = thread.get_robust_list() {
        let _ = Task::block_on(robust_list.wake_all());
        thread.set_robust_list(None);
    }

    unsafe {
        // SAFETY: We are doing execve, all other threads are terminated.
        thread.process.mm_list.replace(Some(load_info.mm_list));
    }

    thread.files.on_exec();
    thread.signal_list.clear_non_ignore();
    thread.set_name(dentry.get_name());

    let mut trap_ctx = thread.trap_ctx.borrow();
    trap_ctx.set_program_counter(load_info.entry_ip.addr());
    trap_ctx.set_stack_pointer(load_info.sp.addr());

    Ok(SyscallNoReturn)
}

#[eonix_macros::define_syscall(SYS_EXIT)]
fn exit(status: u32) -> SyscallNoReturn {
    unsafe {
        let mut procs = Task::block_on(ProcessList::get().write());
        Task::block_on(procs.do_exit(&thread, WaitType::Exited(status), false));
    }

    SyscallNoReturn
}

#[eonix_macros::define_syscall(SYS_EXIT_GROUP)]
fn exit_group(status: u32) -> SyscallNoReturn {
    unsafe {
        let mut procs = Task::block_on(ProcessList::get().write());
        Task::block_on(procs.do_exit(&thread, WaitType::Exited(status), true));
    }

    SyscallNoReturn
}

enum WaitInfo {
    SigInfo(NonNull<SigInfo>),
    Status(NonNull<u32>),
    None,
}

fn do_waitid(
    thread: &Thread,
    id_type: u32,
    _id: u32,
    info: WaitInfo,
    options: u32,
    rusage: *mut RUsage,
) -> KResult<u32> {
    if id_type != P_ALL {
        unimplemented!("waitid with id_type {id_type}");
    }

    if !rusage.is_null() {
        unimplemented!("waitid with rusage pointer");
    }

    let options = match UserWaitOptions::from_bits(options) {
        None => unimplemented!("waitpid with options {options}"),
        Some(options) => options,
    };

    let Some(wait_object) = Task::block_on(thread.process.wait(
        options.contains(UserWaitOptions::WNOHANG),
        options.contains(UserWaitOptions::WUNTRACED),
        options.contains(UserWaitOptions::WCONTINUED),
    ))?
    else {
        return Ok(0);
    };

    match info {
        WaitInfo::SigInfo(siginfo_ptr) => {
            let (status, code) = wait_object.code.to_status_code();

            let mut siginfo = SigInfo::default();
            siginfo.si_pid = wait_object.pid;
            siginfo.si_uid = 0; // All users are root for now.
            siginfo.si_signo = Signal::SIGCHLD.into_raw();
            siginfo.si_status = status;
            siginfo.si_code = code;

            UserPointerMut::new(siginfo_ptr.as_ptr())?.write(siginfo)?;
            Ok(0)
        }
        WaitInfo::Status(status_ptr) => {
            UserPointerMut::new(status_ptr.as_ptr())?.write(wait_object.code.to_wstatus())?;
            Ok(wait_object.pid)
        }
        WaitInfo::None => Ok(wait_object.pid),
    }
}

#[eonix_macros::define_syscall(SYS_WAITID)]
fn waitid(
    id_type: u32,
    id: u32,
    info: *mut SigInfo,
    options: u32,
    rusage: *mut RUsage,
) -> KResult<u32> {
    if let Some(info) = NonNull::new(info) {
        do_waitid(
            thread,
            id_type,
            id,
            WaitInfo::SigInfo(info),
            options,
            rusage,
        )
    } else {
        /*
         * According to POSIX.1-2008, an application calling waitid() must
         * ensure that infop points to a siginfo_t structure (i.e., that it
         * is a non-null pointer).  On Linux, if infop is NULL, waitid()
         * succeeds, and returns the process ID of the waited-for child.
         * Applications should avoid relying on this inconsistent,
         * nonstandard, and unnecessary feature.
         */
        unimplemented!("waitid with null info pointer");
    }
}

#[eonix_macros::define_syscall(SYS_WAIT4)]
fn wait4(waitpid: u32, arg1: *mut u32, options: u32, rusage: *mut RUsage) -> KResult<u32> {
    let waitinfo = if let Some(status) = NonNull::new(arg1) {
        WaitInfo::Status(status)
    } else {
        WaitInfo::None
    };

    let idtype = match waitpid {
        u32::MAX => P_ALL,
        _ => P_PID,
    };

    do_waitid(thread, idtype, waitpid, waitinfo, options, rusage)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_WAITPID)]
fn waitpid(waitpid: u32, arg1: *mut u32, options: u32) -> KResult<u32> {
    sys_wait4(thread, waitpid, arg1, options, core::ptr::null_mut())
}

#[eonix_macros::define_syscall(SYS_SETSID)]
fn setsid() -> KResult<u32> {
    thread.process.setsid()
}

#[eonix_macros::define_syscall(SYS_SETPGID)]
fn setpgid(pid: u32, pgid: i32) -> KResult<()> {
    let pid = if pid == 0 { thread.process.pid } else { pid };

    let pgid = match pgid {
        0 => pid,
        1.. => pgid as u32,
        _ => return Err(EINVAL),
    };

    thread.process.setpgid(pid, pgid)
}

#[eonix_macros::define_syscall(SYS_GETSID)]
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

#[eonix_macros::define_syscall(SYS_GETPGID)]
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

#[eonix_macros::define_syscall(SYS_GETPID)]
fn getpid() -> KResult<u32> {
    Ok(thread.process.pid)
}

#[eonix_macros::define_syscall(SYS_GETPPID)]
fn getppid() -> KResult<u32> {
    Ok(thread.process.parent_rcu().map_or(0, |x| x.pid))
}

fn do_geteuid(_thread: &Thread) -> KResult<u32> {
    // All users are root for now.
    Ok(0)
}

fn do_getuid(_thread: &Thread) -> KResult<u32> {
    // All users are root for now.
    Ok(0)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_GETUID32)]
fn getuid32() -> KResult<u32> {
    do_getuid(thread)
}

#[eonix_macros::define_syscall(SYS_GETUID)]
fn getuid() -> KResult<u32> {
    do_getuid(thread)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_GETEUID32)]
fn geteuid32() -> KResult<u32> {
    do_geteuid(thread)
}

#[eonix_macros::define_syscall(SYS_GETEUID)]
fn geteuid() -> KResult<u32> {
    do_geteuid(thread)
}

#[eonix_macros::define_syscall(SYS_GETGID)]
fn getgid() -> KResult<u32> {
    // All users are root for now.
    Ok(0)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_GETGID32)]
fn getgid32() -> KResult<u32> {
    sys_getgid(thread)
}

#[eonix_macros::define_syscall(SYS_GETTID)]
fn gettid() -> KResult<u32> {
    Ok(thread.tid)
}

pub fn parse_user_tls(arch_tls: usize) -> KResult<UserTLS> {
    #[cfg(target_arch = "x86_64")]
    {
        let desc = arch_tls as *mut posix_types::x86_64::UserDescriptor;
        let desc_pointer = UserPointerMut::new(desc)?;
        let mut desc = desc_pointer.read()?;

        // Clear the TLS area if it is not present.
        if desc.flags.is_read_exec_only() && !desc.flags.is_present() {
            if desc.limit != 0 && desc.base != 0 {
                let len = if desc.flags.is_limit_in_pages() {
                    (desc.limit as usize) << 12
                } else {
                    desc.limit as usize
                };

                CheckedUserPointer::new(desc.base as _, len)?.zero()?;
            }
        }

        let (new_tls, entry) =
            UserTLS::new32(desc.base, desc.limit, desc.flags.is_limit_in_pages());
        desc.entry = entry;
        desc_pointer.write(desc)?;

        Ok(new_tls)
    }

    #[cfg(target_arch = "riscv64")]
    {
        Ok(UserTLS::new(arch_tls as u64))
    }
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_SET_THREAD_AREA)]
fn set_thread_area(arch_tls: usize) -> KResult<()> {
    thread.set_user_tls(parse_user_tls(arch_tls)?)?;

    // SAFETY: Preemption is disabled on calling `load_thread_area32()`.
    unsafe {
        eonix_preempt::disable();
        thread.load_thread_area32();
        eonix_preempt::enable();
    }

    Ok(())
}

#[eonix_macros::define_syscall(SYS_SET_TID_ADDRESS)]
fn set_tid_address(tidptr: usize) -> KResult<u32> {
    thread.clear_child_tid(Some(tidptr));
    Ok(thread.tid)
}

#[eonix_macros::define_syscall(SYS_PRCTL)]
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

#[eonix_macros::define_syscall(SYS_KILL)]
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
            .raise(Signal::try_from_raw(sig)?, procs.prove()),
        // Send signal to the process with the specified pid.
        1.. => procs
            .try_find_process(pid as u32)
            .ok_or(ESRCH)?
            .raise(Signal::try_from_raw(sig)?, procs.prove()),
        // Send signal to the process group with the specified pgid equals to `-pid`.
        ..-1 => procs
            .try_find_pgroup((-pid) as u32)
            .ok_or(ESRCH)?
            .raise(Signal::try_from_raw(sig)?, procs.prove()),
    }

    Ok(())
}

#[eonix_macros::define_syscall(SYS_TKILL)]
fn tkill(tid: u32, sig: u32) -> KResult<()> {
    Task::block_on(ProcessList::get().read())
        .try_find_thread(tid)
        .ok_or(ESRCH)?
        .raise(Signal::try_from_raw(sig)?);
    Ok(())
}

#[eonix_macros::define_syscall(SYS_RT_SIGPROCMASK)]
fn rt_sigprocmask(
    how: u32,
    set: *mut SigSet,
    oldset: *mut SigSet,
    sigsetsize: usize,
) -> KResult<()> {
    if sigsetsize != size_of::<SigSet>() {
        return Err(EINVAL);
    }

    let old_mask = thread.signal_list.get_mask();
    if !oldset.is_null() {
        UserPointerMut::new(oldset)?.write(old_mask)?;
    }

    let new_mask = if !set.is_null() {
        UserPointer::new(set)?.read()?
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

#[eonix_macros::define_syscall(SYS_RT_SIGACTION)]
fn rt_sigaction(
    signum: u32,
    act: *const SigAction,
    oldact: *mut SigAction,
    sigsetsize: usize,
) -> KResult<()> {
    let signal = Signal::try_from_raw(signum)?;
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

#[eonix_macros::define_syscall(SYS_PRLIMIT64)]
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

#[eonix_macros::define_syscall(SYS_GETRLIMIT)]
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

#[eonix_macros::define_syscall(SYS_GETRUSAGE)]
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

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_VFORK)]
fn vfork() -> KResult<u32> {
    let clone_args = CloneArgs::for_vfork();

    do_clone(thread, clone_args)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_FORK)]
fn fork() -> KResult<u32> {
    let clone_args = CloneArgs::for_fork();

    do_clone(thread, clone_args)
}

#[eonix_macros::define_syscall(SYS_CLONE)]
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

#[eonix_macros::define_syscall(SYS_FUTEX)]
fn futex(
    uaddr: usize,
    op: u32,
    val: u32,
    _time_out: usize,
    _uaddr2: usize,
    _val3: u32,
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

#[eonix_macros::define_syscall(SYS_SET_ROBUST_LIST)]
fn set_robust_list(head: usize, len: usize) -> KResult<()> {
    if len != size_of::<RobustListHead>() {
        return Err(EINVAL);
    }

    thread.set_robust_list(Some(VAddr::from(head)));
    Ok(())
}

#[eonix_macros::define_syscall(SYS_RT_SIGRETURN)]
fn rt_sigreturn() -> KResult<SyscallNoReturn> {
    thread
        .signal_list
        .restore(
            &mut thread.trap_ctx.borrow(),
            &mut thread.fpu_state.borrow(),
            false,
        )
        .inspect_err(|err| {
            println_warn!(
                "`rt_sigreturn` failed in thread {} with error {err}!",
                thread.tid
            );
            Task::block_on(thread.force_kill(Signal::SIGSEGV));
        })?;

    Ok(SyscallNoReturn)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_SIGRETURN)]
fn sigreturn() -> KResult<SyscallNoReturn> {
    thread
        .signal_list
        .restore(
            &mut thread.trap_ctx.borrow(),
            &mut thread.fpu_state.borrow(),
            true,
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

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_ARCH_PRCTL)]
fn arch_prctl(option: u32, addr: u32) -> KResult<u32> {
    sys_arch_prctl(thread, option, addr)
}

pub fn keep_alive() {}
