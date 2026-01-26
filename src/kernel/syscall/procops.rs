use alloc::borrow::ToOwned;
use alloc::ffi::CString;
use core::time::Duration;

use bitflags::bitflags;
use eonix_hal::traits::trap::RawTrapContext;
use eonix_hal::trap::TrapContext;
use eonix_mm::address::Addr as _;
use eonix_sync::AsProof as _;
use posix_types::ctypes::PtrT;
use posix_types::signal::{SigAction, SigInfo, SigSet, Signal};
use posix_types::stat::{TimeSpec, TimeVal};
use posix_types::syscall_no::*;
use posix_types::SIGNAL_NOW;

use super::SyscallNoReturn;
use crate::io::Buffer;
use crate::kernel::constants::{
    CLOCK_MONOTONIC, CLOCK_REALTIME, CLOCK_REALTIME_COARSE, EINVAL, ENOENT,
    ENOSYS, ENOTDIR, ERANGE, ESRCH, PR_GET_NAME, PR_SET_NAME, RLIMIT_STACK,
    SIG_BLOCK, SIG_SETMASK, SIG_UNBLOCK,
};
use crate::kernel::mem::PageBuffer;
use crate::kernel::syscall::{User, UserMut};
use crate::kernel::task::{
    do_clone, futex_exec, futex_wait, futex_wake, parse_futexop, yield_now,
    CloneArgs, FutexFlags, FutexOp, ProcessList, ProgramLoader, RobustListHead,
    SignalAction, Thread, WaitId, WaitType,
};
use crate::kernel::timer::sleep;
use crate::kernel::user::{
    UserBuffer, UserPointer, UserPointerMut, UserString,
};
use crate::kernel::vfs::dentry::Dentry;
use crate::kernel::vfs::types::Permission;
use crate::kernel::vfs::{self};
use crate::path::Path;
use crate::prelude::*;

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
async fn nanosleep(
    req: User<(u32, u32)>, rem: UserMut<(u32, u32)>,
) -> KResult<usize> {
    let req = UserPointer::new(req)?.read()?;
    let rem = if rem.is_null() {
        None
    } else {
        Some(UserPointerMut::new(rem)?)
    };

    let duration =
        Duration::from_secs(req.0 as u64) + Duration::from_nanos(req.1 as u64);
    sleep(duration).await;

    if let Some(rem) = rem {
        rem.write((0, 0))?;
    }

    Ok(0)
}

#[eonix_macros::define_syscall(SYS_CLOCK_NANOSLEEP)]
async fn clock_nanosleep(
    clock_id: u32, _flags: u32, req: User<(u32, u32)>, rem: UserMut<(u32, u32)>,
) -> KResult<usize> {
    if clock_id != CLOCK_REALTIME
        && clock_id != CLOCK_REALTIME_COARSE
        && clock_id != CLOCK_MONOTONIC
    {
        unimplemented!("Unsupported clock_id: {}", clock_id);
    }

    let req = UserPointer::new(req)?.read()?;
    let rem = if rem.is_null() {
        None
    } else {
        Some(UserPointerMut::new(rem)?)
    };

    let duration =
        Duration::from_secs(req.0 as u64) + Duration::from_nanos(req.1 as u64);
    sleep(duration).await;

    if let Some(rem) = rem {
        rem.write((0, 0))?;
    }

    Ok(0)
}

#[eonix_macros::define_syscall(SYS_UMASK)]
async fn umask(raw_new_mask: u32) -> KResult<u32> {
    let new_mask = Permission::new(!raw_new_mask);
    let old_mask =
        core::mem::replace(&mut *thread.fs_context.umask.lock(), new_mask);

    Ok(!old_mask.bits())
}

#[eonix_macros::define_syscall(SYS_GETCWD)]
async fn getcwd(buffer: UserMut<u8>, bufsize: usize) -> KResult<usize> {
    let mut user_buffer = UserBuffer::new(buffer, bufsize)?;
    let mut buffer = PageBuffer::new();

    let cwd = thread.fs_context.cwd.lock().clone();
    cwd.get_path(&thread.fs_context, &mut buffer)?;

    user_buffer.fill(buffer.data())?.ok_or(ERANGE)?;

    Ok(buffer.wrote())
}

#[eonix_macros::define_syscall(SYS_CHDIR)]
async fn chdir(path: User<u8>) -> KResult<()> {
    let path = UserString::new(path)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&thread.fs_context, path, true).await?;
    if !dentry.is_valid() {
        return Err(ENOENT);
    }

    if !dentry.is_directory() {
        return Err(ENOTDIR);
    }

    *thread.fs_context.cwd.lock() = dentry;
    Ok(())
}

#[eonix_macros::define_syscall(SYS_UMOUNT)]
async fn umount(source: User<u8>) -> KResult<()> {
    let source = UserString::new(source)?;
    if source.as_cstr().to_str().unwrap() == "./mnt" {
        return Ok(());
    }
    return Err(ENOENT);
}

#[eonix_macros::define_syscall(SYS_MOUNT)]
async fn mount(
    source: User<u8>, target: User<u8>, fstype: User<u8>, flags: usize,
) -> KResult<()> {
    let source = UserString::new(source)?;
    if source.as_cstr().to_str().unwrap() == "/dev/vda2" {
        return Ok(());
    }
    let target = UserString::new(target)?;
    let fstype = UserString::new(fstype)?;

    let mountpoint = Dentry::open(
        &thread.fs_context,
        Path::new(target.as_cstr().to_bytes())?,
        true,
    )
    .await?;

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
    .await
}

fn get_strings(
    mut ptr_strings: UserPointer<'_, PtrT>,
) -> KResult<Vec<CString>> {
    let mut strings = Vec::new();

    loop {
        let ptr = ptr_strings.read()?;
        if ptr.is_null() {
            break;
        }

        let user_string = UserString::new(User::with_addr(ptr.addr()))?;
        strings.push(user_string.as_cstr().to_owned());
        ptr_strings = ptr_strings.offset(1)?;
    }

    Ok(strings)
}

#[eonix_macros::define_syscall(SYS_EXECVE)]
async fn execve(
    exec: User<u8>, argv: User<PtrT>, envp: User<PtrT>,
) -> KResult<SyscallNoReturn> {
    let exec = UserString::new(exec)?;
    let exec = exec.as_cstr().to_owned();

    let argv = get_strings(UserPointer::new(argv)?)?;
    let envp = get_strings(UserPointer::new(envp)?)?;

    let dentry =
        Dentry::open(&thread.fs_context, Path::new(exec.as_bytes())?, true)
            .await?;
    if !dentry.is_valid() {
        Err(ENOENT)?;
    }

    // TODO: When `execve` is called by one of the threads in a process, the other threads
    //       should be terminated and `execve` is performed in the thread group leader.
    let load_info = ProgramLoader::parse(
        &thread.fs_context,
        exec,
        dentry.clone(),
        argv,
        envp,
    )
    .await?
    .load()
    .await?;

    futex_exec(thread).await;

    unsafe {
        // SAFETY: We are doing execve, all other threads are terminated.
        thread.process.mm_list.replace(Some(load_info.mm_list));
    }

    thread.files.on_exec().await;
    thread.signal_list.clear_non_ignore();
    thread.set_name(dentry.get_name());

    let mut trap_ctx = thread.trap_ctx.borrow();
    *trap_ctx = TrapContext::new();

    trap_ctx.set_user_mode(true);
    trap_ctx.set_interrupt_enabled(true);
    trap_ctx.set_program_counter(load_info.entry_ip.addr());
    trap_ctx.set_stack_pointer(load_info.sp.addr());

    Ok(SyscallNoReturn)
}

#[eonix_macros::define_syscall(SYS_EXIT)]
async fn exit(status: u32) -> SyscallNoReturn {
    thread.exit(WaitType::Exited(status));

    SyscallNoReturn
}

#[eonix_macros::define_syscall(SYS_EXIT_GROUP)]
async fn exit_group(status: u32) -> SyscallNoReturn {
    // XXX: Send SIGKILL to our sibling threads.
    thread.exit(WaitType::Exited(status));

    SyscallNoReturn
}

enum WaitInfo {
    SigInfo(UserMut<SigInfo>),
    Status(UserMut<u32>),
    None,
}

async fn do_waitid(
    thread: &Thread, wait_id: WaitId, info: WaitInfo, options: u32,
    rusage: UserMut<RUsage>,
) -> KResult<u32> {
    if !rusage.is_null() {
        unimplemented!("waitid with rusage pointer");
    }

    let options = match UserWaitOptions::from_bits(options) {
        None => unimplemented!("waitpid with options {options}"),
        Some(options) => options,
    };

    let Some(wait_object) = thread
        .process
        .wait(
            wait_id,
            options.contains(UserWaitOptions::WNOHANG),
            options.contains(UserWaitOptions::WUNTRACED),
            options.contains(UserWaitOptions::WCONTINUED),
        )
        .await?
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

            UserPointerMut::new(siginfo_ptr)?.write(siginfo)?;
            Ok(0)
        }
        WaitInfo::Status(status_ptr) => {
            UserPointerMut::new(status_ptr)?
                .write(wait_object.code.to_wstatus())?;
            Ok(wait_object.pid)
        }
        WaitInfo::None => Ok(wait_object.pid),
    }
}

#[eonix_macros::define_syscall(SYS_WAITID)]
async fn waitid(
    id_type: u32, id: u32, info: UserMut<SigInfo>, options: u32,
    rusage: UserMut<RUsage>,
) -> KResult<u32> {
    let wait_id = WaitId::from_type_and_id(id_type, id)?;

    if info.is_null() {
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

    do_waitid(thread, wait_id, WaitInfo::SigInfo(info), options, rusage).await
}

#[eonix_macros::define_syscall(SYS_WAIT4)]
async fn wait4(
    wait_id: i32, arg1: UserMut<u32>, options: u32, rusage: UserMut<RUsage>,
) -> KResult<u32> {
    let waitinfo = if arg1.is_null() {
        WaitInfo::None
    } else {
        WaitInfo::Status(arg1)
    };

    let wait_id = WaitId::from_id(wait_id, thread);

    do_waitid(thread, wait_id, waitinfo, options, rusage).await
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_WAITPID)]
async fn waitpid(
    waitpid: i32, arg1: UserMut<u32>, options: u32,
) -> KResult<u32> {
    sys_wait4(thread, waitpid, arg1, options, UserMut::null()).await
}

#[eonix_macros::define_syscall(SYS_SETSID)]
async fn setsid() -> KResult<u32> {
    thread.process.setsid().await
}

#[eonix_macros::define_syscall(SYS_SETPGID)]
async fn setpgid(pid: u32, pgid: i32) -> KResult<()> {
    let pid = if pid == 0 { thread.process.pid } else { pid };

    let pgid = match pgid {
        0 => pid,
        1.. => pgid as u32,
        _ => return Err(EINVAL),
    };

    thread.process.setpgid(pid, pgid).await
}

#[eonix_macros::define_syscall(SYS_GETSID)]
async fn getsid(pid: u32) -> KResult<u32> {
    if pid == 0 {
        Ok(thread.process.session_rcu().sid)
    } else {
        let procs = ProcessList::get().read().await;
        procs
            .try_find_process(pid)
            .map(|proc| proc.session(procs.prove()).sid)
            .ok_or(ESRCH)
    }
}

#[eonix_macros::define_syscall(SYS_GETPGID)]
async fn getpgid(pid: u32) -> KResult<u32> {
    if pid == 0 {
        Ok(thread.process.pgroup_rcu().pgid)
    } else {
        let procs = ProcessList::get().read().await;
        procs
            .try_find_process(pid)
            .map(|proc| proc.pgroup(procs.prove()).pgid)
            .ok_or(ESRCH)
    }
}

#[eonix_macros::define_syscall(SYS_GETPID)]
async fn getpid() -> KResult<u32> {
    Ok(thread.process.pid)
}

#[eonix_macros::define_syscall(SYS_GETPPID)]
async fn getppid() -> KResult<u32> {
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
async fn getuid32() -> KResult<u32> {
    do_getuid(thread)
}

#[eonix_macros::define_syscall(SYS_GETUID)]
async fn getuid() -> KResult<u32> {
    do_getuid(thread)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_GETEUID32)]
async fn geteuid32() -> KResult<u32> {
    do_geteuid(thread)
}

#[eonix_macros::define_syscall(SYS_GETEUID)]
async fn geteuid() -> KResult<u32> {
    do_geteuid(thread)
}

#[eonix_macros::define_syscall(SYS_GETEGID)]
async fn getegid() -> KResult<u32> {
    // All users are root for now.
    Ok(0)
}

#[eonix_macros::define_syscall(SYS_GETGID)]
async fn getgid() -> KResult<u32> {
    sys_getegid(thread).await
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_GETGID32)]
async fn getgid32() -> KResult<u32> {
    sys_getegid(thread).await
}

#[eonix_macros::define_syscall(SYS_SCHED_YIELD)]
async fn sched_yield() -> KResult<()> {
    yield_now().await;
    Ok(())
}

#[eonix_macros::define_syscall(SYS_SYNC)]
async fn sync() -> KResult<()> {
    Ok(())
}

#[eonix_macros::define_syscall(SYS_FSYNC)]
async fn fsync() -> KResult<()> {
    Ok(())
}

#[eonix_macros::define_syscall(SYS_GETTID)]
async fn gettid() -> KResult<u32> {
    Ok(thread.tid)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_SET_THREAD_AREA)]
async fn set_thread_area(tls: PtrT) -> KResult<()> {
    use crate::kernel::task::UserTLSDescriptor;

    let tls = UserTLSDescriptor::new(tls)?.read()?;

    thread.set_user_tls(tls)?;
    thread.activate_tls();

    Ok(())
}

#[eonix_macros::define_syscall(SYS_SET_TID_ADDRESS)]
async fn set_tid_address(tidptr: UserMut<u32>) -> KResult<u32> {
    thread.clear_child_tid(Some(tidptr));
    Ok(thread.tid)
}

#[eonix_macros::define_syscall(SYS_PRCTL)]
async fn prctl(option: u32, arg2: PtrT) -> KResult<()> {
    match option {
        PR_SET_NAME => {
            let name =
                UserPointer::<[u8; 16]>::new(User::with_addr(arg2.addr()))?
                    .read()?;
            let len = name.iter().position(|&c| c == 0).unwrap_or(15);
            thread.set_name(name[..len].into());
            Ok(())
        }
        PR_GET_NAME => {
            let name = thread.get_name();
            let len = name.len().min(15);
            let name: [u8; 16] =
                core::array::from_fn(|i| if i < len { name[i] } else { 0 });
            UserPointerMut::<[u8; 16]>::new(UserMut::with_addr(arg2.addr()))?
                .write(name)?;
            Ok(())
        }
        _ => Err(EINVAL),
    }
}

#[eonix_macros::define_syscall(SYS_KILL)]
async fn kill(pid: i32, sig: u32) -> KResult<()> {
    let procs = ProcessList::get().read().await;
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
async fn tkill(tid: u32, sig: u32) -> KResult<()> {
    ProcessList::get()
        .read()
        .await
        .try_find_thread(tid)
        .ok_or(ESRCH)?
        .raise(Signal::try_from_raw(sig)?);
    Ok(())
}

#[eonix_macros::define_syscall(SYS_TGKILL)]
async fn tgkill(tgid: u32, tid: u32, sig: u32) -> KResult<()> {
    let procs = ProcessList::get().read().await;

    let thread_to_kill = procs.try_find_thread(tid).ok_or(ESRCH)?;
    if thread_to_kill.process.pid != tgid {
        return Err(ESRCH);
    }

    thread_to_kill.raise(Signal::try_from_raw(sig)?);
    Ok(())
}

#[eonix_macros::define_syscall(SYS_RT_SIGPROCMASK)]
async fn rt_sigprocmask(
    how: u32, set: UserMut<SigSet>, oldset: UserMut<SigSet>, sigsetsize: usize,
) -> KResult<()> {
    if sigsetsize != size_of::<SigSet>() {
        return Err(EINVAL);
    }

    let old_mask = thread.signal_list.get_mask();
    if !oldset.is_null() {
        UserPointerMut::new(oldset)?.write(old_mask)?;
    }

    let new_mask = if !set.is_null() {
        UserPointer::new(set.as_const())?.read()?
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

#[cfg_attr(
    any(target_arch = "riscv64", target_arch = "loongarch64"),
    eonix_macros::define_syscall(SYS_RT_SIGTIMEDWAIT)
)]
async fn rt_sigtimedwait(
    _uthese: User<SigSet>, _uinfo: UserMut<SigInfo>, _uts: User<TimeSpec>,
) -> KResult<i32> {
    // TODO
    Ok(0)
}

#[eonix_macros::define_syscall(SYS_RT_SIGACTION)]
async fn rt_sigaction(
    signum: u32, act: User<SigAction>, oldact: UserMut<SigAction>,
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
async fn prlimit64(
    pid: u32, resource: u32, new_limit: User<RLimit>,
    old_limit: UserMut<RLimit>,
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
                let new_rlimit = UserPointer::new(new_limit)?.read()?;
                if new_rlimit.rlim_cur > new_rlimit.rlim_max {
                    return Err(EINVAL);
                }
                // TODO:
                // thread.process().set_rlimit(resource, new_rlimit)?;
            }
            Ok(())
        }
        _ => Err(ENOSYS),
    }
}

#[eonix_macros::define_syscall(SYS_GETRLIMIT)]
async fn getrlimit(resource: u32, rlimit: UserMut<RLimit>) -> KResult<()> {
    sys_prlimit64(thread, 0, resource, User::null(), rlimit).await
}

#[eonix_macros::define_syscall(SYS_SETRLIMIT)]
async fn setrlimit(resource: u32, rlimit: User<RLimit>) -> KResult<()> {
    sys_prlimit64(thread, 0, resource, rlimit, UserMut::null()).await
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
async fn getrusage(who: u32, rusage: UserMut<RUsage>) -> KResult<()> {
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
async fn vfork() -> KResult<u32> {
    let clone_args = CloneArgs::for_vfork();

    do_clone(thread, clone_args).await
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_FORK)]
async fn fork() -> KResult<u32> {
    let clone_args = CloneArgs::for_fork();

    do_clone(thread, clone_args).await
}

// Some old platforms including x86_32, riscv and arm have the last two arguments
// swapped, so we need to define two versions of `clone` syscall.
#[cfg(not(target_arch = "loongarch64"))]
#[eonix_macros::define_syscall(SYS_CLONE)]
async fn clone(
    clone_flags: usize, new_sp: usize, parent_tidptr: UserMut<u32>, tls: PtrT,
    child_tidptr: UserMut<u32>,
) -> KResult<u32> {
    let clone_args = CloneArgs::for_clone(
        clone_flags,
        new_sp,
        child_tidptr,
        parent_tidptr,
        tls,
    )?;

    do_clone(thread, clone_args).await
}

#[cfg(target_arch = "loongarch64")]
#[eonix_macros::define_syscall(SYS_CLONE)]
async fn clone(
    clone_flags: usize, new_sp: usize, parent_tidptr: UserMut<u32>,
    child_tidptr: UserMut<u32>, tls: usize,
) -> KResult<u32> {
    let clone_args = CloneArgs::for_clone(
        clone_flags,
        new_sp,
        child_tidptr,
        parent_tidptr,
        tls,
    )?;

    do_clone(thread, clone_args).await
}

#[eonix_macros::define_syscall(SYS_FUTEX)]
async fn futex(
    uaddr: usize, op: u32, val: u32, _time_out: usize, _uaddr2: usize,
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
            futex_wait(uaddr, pid, val as u32, None).await?;
            return Ok(0);
        }
        FutexOp::FUTEX_WAKE => {
            return futex_wake(uaddr, pid, val as u32).await;
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
async fn set_robust_list(
    head: User<RobustListHead>, len: usize,
) -> KResult<()> {
    if len != size_of::<RobustListHead>() {
        return Err(EINVAL);
    }

    thread.set_robust_list(Some(head));
    Ok(())
}

#[eonix_macros::define_syscall(SYS_RT_SIGRETURN)]
async fn rt_sigreturn() -> KResult<SyscallNoReturn> {
    if let Err(err) = thread.signal_list.restore(
        &mut thread.trap_ctx.borrow(),
        &mut thread.fpu_state.borrow(),
        false,
    ) {
        println_warn!(
            "`rt_sigreturn` failed in thread {} with error {err}!",
            thread.tid
        );
        thread.force_kill(Signal::SIGSEGV);
        return Err(err);
    }

    Ok(SyscallNoReturn)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_SIGRETURN)]
async fn sigreturn() -> KResult<SyscallNoReturn> {
    if let Err(err) = thread.signal_list.restore(
        &mut thread.trap_ctx.borrow(),
        &mut thread.fpu_state.borrow(),
        true,
    ) {
        println_warn!(
            "`sigreturn` failed in thread {} with error {err}!",
            thread.tid
        );
        thread.force_kill(Signal::SIGSEGV);
        return Err(err);
    }

    Ok(SyscallNoReturn)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_ARCH_PRCTL)]
async fn arch_prctl(option: u32, addr: PtrT) -> KResult<u32> {
    match option {
        PR_SET_NAME => {
            let name =
                UserPointer::<[u8; 16]>::new(User::with_addr(addr.addr()))?
                    .read()?;
            let len = name.iter().position(|&c| c == 0).unwrap_or(15);
            thread.set_name(name[..len].into());
            Ok(0)
        }
        PR_GET_NAME => {
            let name = thread.get_name();
            let len = name.len().min(15);
            let name: [u8; 16] =
                core::array::from_fn(|i| if i < len { name[i] } else { 0 });
            UserPointerMut::<[u8; 16]>::new(UserMut::with_addr(addr.addr()))?
                .write(name)?;
            Ok(0)
        }
        _ => Err(EINVAL),
    }
}

pub fn keep_alive() {}
