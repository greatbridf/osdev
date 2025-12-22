use core::num::NonZero;

use bitflags::bitflags;
use eonix_runtime::scheduler::RUNTIME;
use eonix_sync::AsProof;
use posix_types::ctypes::PtrT;
use posix_types::signal::Signal;

use super::{UserTLS, UserTLSDescriptor};
use crate::kernel::syscall::UserMut;
use crate::kernel::task::{alloc_pid, ProcessBuilder, ProcessList, Thread, ThreadBuilder};
use crate::kernel::user::UserPointerMut;
use crate::KResult;

bitflags! {
    #[derive(Debug, Default)]
    pub struct CloneFlags: usize {
        const CLONE_VM      = 0x00000100;       /* Set if VM shared between processes.  */
        const CLONE_FS      = 0x00000200;       /* Set if fs info shared between processes.  */
        const CLONE_FILES   = 0x00000400;       /* Set if open files shared between processes.  */
        const CLONE_SIGHAND = 0x00000800;       /* Set if signal handlers shared.  */
        const CLONE_PIDFD   = 0x00001000;       /* Set if a pidfd should be placed in parent.  */
        const CLONE_PTRACE  = 0x00002000;       /* Set if tracing continues on the child.  */
        const CLONE_VFORK   = 0x00004000;       /* Set if the parent wants the child to wake it up on mm_release.  */
        const CLONE_PARENT  = 0x00008000;       /* Set if we want to have the same parent as the cloner.  */
        const CLONE_THREAD  = 0x00010000;       /* Set to add to same thread group.  */
        const CLONE_NEWNS   = 0x00020000;       /* Set to create new namespace.  */
        const CLONE_SYSVSEM = 0x00040000;       /* Set to shared SVID SEM_UNDO semantics.  */
        const CLONE_SETTLS  = 0x00080000;       /* Set TLS info.  */
        const CLONE_PARENT_SETTID = 0x00100000; /* Store TID in userlevel buffer before MM copy.  */
        const CLONE_CHILD_CLEARTID = 0x00200000;/* Register exit futex and memory location to clear.  */
        const CLONE_DETACHED = 0x00400000;      /* Create clone detached.  */
        const CLONE_UNTRACED = 0x00800000;      /* Set if the tracing process can't force CLONE_PTRACE on this clone.  */
        const CLONE_CHILD_SETTID = 0x01000000;  /* Store TID in userlevel buffer in the child.  */
        const CLONE_NEWCGROUP   = 0x02000000;	/* New cgroup namespace.  */
        const CLONE_NEWUTS	= 0x04000000;	    /* New utsname group.  */
        const CLONE_NEWIPC	= 0x08000000;	    /* New ipcs.  */
        const CLONE_NEWUSER	= 0x10000000;	    /* New user namespace.  */
        const CLONE_NEWPID	= 0x20000000;	    /* New pid namespace.  */
        const CLONE_NEWNET	= 0x40000000;	    /* New network namespace.  */
        const CLONE_IO	= 0x80000000;	        /* Clone I/O context.  */
    }
}

#[derive(Debug)]
pub struct CloneArgs {
    pub flags: CloneFlags,
    /// Stack pointer for the new thread.
    pub sp: Option<NonZero<usize>>,
    /// Signal to send to the parent on exit.
    pub exit_signal: Option<Signal>,
    /// Pointer to set child TID in user space.
    pub set_tid_ptr: Option<UserMut<u32>>,
    /// Pointer to clear child TID in user space.
    pub clear_tid_ptr: Option<UserMut<u32>>,
    /// Pointer to parent TID in user space.
    pub parent_tid_ptr: Option<UserMut<u32>>,
    /// Pointer to TLS information.
    pub tls: Option<UserTLS>,
}

impl CloneArgs {
    const MASK: usize = 0xff;

    pub fn for_clone(
        flags: usize,
        sp: usize,
        child_tid_ptr: UserMut<u32>,
        parent_tid_ptr: UserMut<u32>,
        tls: PtrT,
    ) -> KResult<Self> {
        let clone_flags = CloneFlags::from_bits_truncate(flags & !Self::MASK);
        let exit_signal = flags & Self::MASK;
        let exit_signal = if exit_signal != 0 {
            Some(Signal::try_from_raw(exit_signal as u32)?)
        } else {
            None
        };

        let mut set_tid_ptr = None;
        if clone_flags.contains(CloneFlags::CLONE_CHILD_SETTID) {
            set_tid_ptr = Some(child_tid_ptr);
        }

        let mut clear_tid_ptr = None;
        if clone_flags.contains(CloneFlags::CLONE_CHILD_CLEARTID) {
            clear_tid_ptr = Some(child_tid_ptr);
        }

        let parent_tid_ptr = clone_flags
            .contains(CloneFlags::CLONE_PARENT_SETTID)
            .then_some(parent_tid_ptr);

        let tls = if clone_flags.contains(CloneFlags::CLONE_SETTLS) {
            let tls_desc = UserTLSDescriptor::new(tls)?;
            Some(tls_desc.read()?)
        } else {
            None
        };

        let clone_args = CloneArgs {
            flags: clone_flags,
            sp: NonZero::new(sp),
            set_tid_ptr,
            clear_tid_ptr,
            parent_tid_ptr,
            exit_signal,
            tls,
        };

        Ok(clone_args)
    }

    pub fn for_fork() -> Self {
        CloneArgs {
            flags: CloneFlags::empty(),
            sp: None,
            set_tid_ptr: None,
            clear_tid_ptr: None,
            parent_tid_ptr: None,
            exit_signal: Some(Signal::SIGCHLD),
            tls: None,
        }
    }

    pub fn for_vfork() -> Self {
        CloneArgs {
            flags: CloneFlags::CLONE_VFORK | CloneFlags::CLONE_VM,
            sp: None,
            set_tid_ptr: None,
            clear_tid_ptr: None,
            parent_tid_ptr: None,
            exit_signal: Some(Signal::SIGCHLD),
            tls: None,
        }
    }
}

pub async fn do_clone(thread: &Thread, clone_args: CloneArgs) -> KResult<u32> {
    let mut procs = ProcessList::get().write().await;

    let thread_builder = ThreadBuilder::new().clone_from(&thread, &clone_args)?;
    let current_process = thread.process.clone();

    let new_pid = alloc_pid();

    let new_thread = if clone_args.flags.contains(CloneFlags::CLONE_THREAD) {
        let new_thread = thread_builder
            .process(current_process)
            .tid(new_pid)
            .tls(clone_args.tls)
            .build(&mut procs);
        new_thread
    } else {
        let current_pgroup = current_process.pgroup(procs.prove()).clone();
        let current_session = current_process.session(procs.prove()).clone();

        let (new_thread, _) = ProcessBuilder::new()
            .clone_from(current_process, &clone_args)
            .await
            .pid(new_pid)
            .pgroup(current_pgroup)
            .session(current_session)
            .thread_builder(thread_builder)
            .build(&mut procs);
        new_thread
    };

    if let Some(parent_tid_ptr) = clone_args.parent_tid_ptr {
        UserPointerMut::new(parent_tid_ptr)?.write(new_pid)?
    }

    RUNTIME.spawn(new_thread.run());

    Ok(new_pid)
}
