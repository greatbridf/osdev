use alloc::collections::vec_deque::VecDeque;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, Ordering};

use eonix_sync::{
    AsProof as _, AsProofMut as _, Locked, Proof, ProofMut, RwLockReadGuard,
    SpinGuard, UnlockableGuard as _, UnlockedGuard as _,
};
use intrusive_collections::{
    intrusive_adapter, KeyAdapter, RBTree, RBTreeAtomicLink,
};
use pointers::BorrowedArc;
use posix_types::constants::{
    CLD_CONTINUED, CLD_DUMPED, CLD_EXITED, CLD_KILLED, CLD_STOPPED, P_ALL,
    P_PGID, P_PID, P_PIDFD,
};
use posix_types::signal::Signal;
use posix_types::SIGNAL_COREDUMP;

use super::signal::RaiseResult;
use super::thread::{ProcessThreads, ThreadBuilder};
use super::{ProcessGroup, ProcessList, Session, Thread};
use crate::kernel::constants::{ECHILD, EINTR, EINVAL, EPERM, ESRCH};
use crate::kernel::mem::MMList;
use crate::kernel::task::{CloneArgs, CloneFlags};
use crate::prelude::*;
use crate::rcu::{call_rcu, RCUPointer, RCUReadGuard};
use crate::sync::CondVar;

pub struct ProcessBuilder {
    mm_list: Option<MMList>,
    exit_signal: Option<Signal>,
    parent: Option<Arc<Process>>,
    thread_builder: Option<ThreadBuilder>,
    pgroup: Option<Arc<ProcessGroup>>,
    session: Option<Arc<Session>>,
    pid: Option<u32>,
}

pub struct Process {
    /// Process id
    ///
    /// This should never change during the life of the process.
    pub pid: u32,

    pub wait_list: WaitList,
    pub mm_list: MMList,

    pub exit_signal: Option<Signal>,

    /// Parent process
    ///
    /// `parent` must be valid during the whole life of the process.
    /// The only case where it may be `None` is when it is the init process
    /// or the process is kernel thread.
    pub(super) parent: RCUPointer<Process>,

    /// Process group
    ///
    /// `pgroup` must be valid during the whole life of the process.
    /// The only case where it may be `None` is when the process is kernel thread.
    pub(super) pgroup: RCUPointer<ProcessGroup>,

    /// Session
    ///
    /// `session` must be valid during the whole life of the process.
    /// The only case where it may be `None` is when the process is kernel thread.
    pub(super) session: RCUPointer<Session>,

    pub children: Locked<RBTree<ProcessChildren>, ProcessList>,
    pub threads: Locked<RBTree<ProcessThreads>, ProcessList>,

    all_procs_link: RBTreeAtomicLink,
    group_procs_link: RBTreeAtomicLink,
    siblings_link: RBTreeAtomicLink,
}

intrusive_adapter!(pub AllProcs = Arc<Process>: Process {
    all_procs_link: RBTreeAtomicLink
});
intrusive_adapter!(pub GroupProcs = Arc<Process>: Process {
    group_procs_link: RBTreeAtomicLink
});
intrusive_adapter!(pub ProcessChildren = Arc<Process>: Process {
    siblings_link: RBTreeAtomicLink
});

impl KeyAdapter<'_> for AllProcs {
    type Key = u32;

    fn get_key(
        &self,
        value: &'_ <Self::PointerOps as intrusive_collections::PointerOps>::Value,
    ) -> Self::Key {
        value.pid
    }
}

impl KeyAdapter<'_> for GroupProcs {
    type Key = u32;

    fn get_key(
        &self,
        value: &'_ <Self::PointerOps as intrusive_collections::PointerOps>::Value,
    ) -> Self::Key {
        value.pid
    }
}

impl KeyAdapter<'_> for ProcessChildren {
    type Key = u32;

    fn get_key(
        &self,
        value: &'_ <Self::PointerOps as intrusive_collections::PointerOps>::Value,
    ) -> Self::Key {
        value.pid
    }
}

#[derive(Debug)]
pub struct WaitList {
    wait_procs: Spin<VecDeque<WaitObject>>,
    cv_wait_procs: CondVar,
}

pub struct NotifyBatch<'waitlist, 'process, 'cv> {
    wait_procs: SpinGuard<'waitlist, VecDeque<WaitObject>>,
    process: &'process Process,
    cv: &'cv CondVar,
    needs_notify: bool,
}

pub struct Entry<'waitlist, 'proclist, 'cv> {
    wait_procs: SpinGuard<'waitlist, VecDeque<WaitObject>>,
    process_list: RwLockReadGuard<'proclist, ProcessList>,
    cv: &'cv CondVar,
    want_stop: bool,
    want_continue: bool,
    want_id: WaitId,
}

pub struct DrainExited<'waitlist> {
    wait_procs: SpinGuard<'waitlist, VecDeque<WaitObject>>,
}

#[derive(Debug, Clone, Copy)]
pub enum WaitId {
    Any,
    Pid(u32),
    Pgid(u32),
}

impl WaitId {
    pub fn from_type_and_id(id_type: u32, id: u32) -> KResult<Self> {
        match id_type {
            P_ALL => Ok(WaitId::Any),
            P_PID => Ok(WaitId::Pid(id)),
            P_PGID => Ok(WaitId::Pgid(id)),
            P_PIDFD => panic!("P_PIDFD type is not supported"),
            _ => Err(EINVAL),
        }
    }

    pub fn from_id(id: i32, thread: &Thread) -> Self {
        match id {
            ..-1 => WaitId::Pgid((-id).cast_unsigned()),
            -1 => WaitId::Any,
            0 => WaitId::Pgid(thread.process.pgroup_rcu().pgid),
            _ => WaitId::Pid(id.cast_unsigned()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitType {
    Exited(u32),
    Signaled(Signal),
    Stopped(Signal),
    Continued,
}

#[derive(Debug, Clone, Copy)]
pub struct WaitObject {
    pub pid: u32,
    pub code: WaitType,
}

impl WaitType {
    pub fn to_wstatus(self) -> u32 {
        match self {
            WaitType::Exited(status) => (status & 0xff) << 8,
            WaitType::Signaled(signal @ SIGNAL_COREDUMP!()) => {
                signal.into_raw() | 0x80
            }
            WaitType::Signaled(signal) => signal.into_raw(),
            WaitType::Stopped(signal) => 0x7f | (signal.into_raw() << 8),
            WaitType::Continued => 0xffff,
        }
    }

    pub fn to_status_code(self) -> (u32, u32) {
        // TODO: CLD_TRAPPED
        match self {
            WaitType::Exited(status) => (status, CLD_EXITED),
            WaitType::Signaled(signal @ SIGNAL_COREDUMP!()) => {
                (signal.into_raw(), CLD_DUMPED)
            }
            WaitType::Signaled(signal) => (signal.into_raw(), CLD_KILLED),
            WaitType::Stopped(signal) => (signal.into_raw(), CLD_STOPPED),
            WaitType::Continued => (Signal::SIGCONT.into_raw(), CLD_CONTINUED),
        }
    }
}

impl WaitObject {
    pub fn stopped(&self) -> Option<Signal> {
        if let WaitType::Stopped(signal) = self.code {
            Some(signal)
        } else {
            None
        }
    }

    pub fn is_continue(&self) -> bool {
        matches!(self.code, WaitType::Continued)
    }
}

impl ProcessBuilder {
    pub fn new() -> Self {
        Self {
            pid: None,
            mm_list: None,
            exit_signal: None,
            parent: None,
            thread_builder: None,
            pgroup: None,
            session: None,
        }
    }

    pub async fn clone_from(
        mut self,
        process: Arc<Process>,
        clone_args: &CloneArgs,
    ) -> Self {
        let mm_list = if clone_args.flags.contains(CloneFlags::CLONE_VM) {
            process.mm_list.new_shared().await
        } else {
            process.mm_list.new_cloned().await
        };

        if let Some(exit_signal) = clone_args.exit_signal {
            self = self.exit_signal(exit_signal)
        }

        self.mm_list(mm_list).parent(process)
    }

    pub fn exit_signal(mut self, exit_signal: Signal) -> Self {
        self.exit_signal = Some(exit_signal);
        self
    }

    pub fn mm_list(mut self, mm_list: MMList) -> Self {
        self.mm_list = Some(mm_list);
        self
    }

    pub fn pid(mut self, pid: u32) -> Self {
        self.pid = Some(pid);
        self
    }

    pub fn parent(mut self, parent: Arc<Process>) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn thread_builder(mut self, thread_builder: ThreadBuilder) -> Self {
        self.thread_builder = Some(thread_builder);
        self
    }

    pub fn pgroup(mut self, pgroup: Arc<ProcessGroup>) -> Self {
        self.pgroup = Some(pgroup);
        self
    }

    pub fn session(mut self, session: Arc<Session>) -> Self {
        self.session = Some(session);
        self
    }

    pub fn build(
        self,
        process_list: &mut ProcessList,
    ) -> (Arc<Thread>, Arc<Process>) {
        let mm_list = self.mm_list.unwrap_or_else(|| MMList::new());

        let process = Arc::new(Process {
            pid: self.pid.expect("should set pid before building"),
            wait_list: WaitList::new(),
            mm_list,
            exit_signal: self.exit_signal,
            parent: RCUPointer::empty(),
            pgroup: RCUPointer::empty(),
            session: RCUPointer::empty(),
            children: Locked::new(
                RBTree::new(ProcessChildren::NEW),
                process_list,
            ),
            threads: Locked::new(
                RBTree::new(ProcessThreads::NEW),
                process_list,
            ),
            all_procs_link: RBTreeAtomicLink::new(),
            group_procs_link: RBTreeAtomicLink::new(),
            siblings_link: RBTreeAtomicLink::new(),
        });

        process_list.add_process(&process);

        let thread_builder =
            self.thread_builder.expect("Thread builder is not set");
        let thread = thread_builder
            .process(process.clone())
            .tid(process.pid)
            .build(process_list);

        let session = match self.session {
            Some(session) => session,
            None => Session::new(&process, process_list),
        };

        let pgroup = match self.pgroup {
            Some(pgroup) => {
                pgroup.add_member(&process, process_list.prove_mut());
                pgroup
            }
            None => ProcessGroup::new(&process, &session, process_list),
        };

        if let Some(parent) = &self.parent {
            parent.add_child(&process, process_list.prove_mut());
        }

        // SAFETY: We are holding the process list lock.
        unsafe {
            process.parent.swap(self.parent);
            process.pgroup.swap(Some(pgroup));
            process.session.swap(Some(session));
        }

        (thread, process)
    }
}

impl Process {
    pub fn raise(&self, signal: Signal, procs: Proof<'_, ProcessList>) {
        let threads = self.threads.access(procs);
        for thread in threads.iter() {
            if let RaiseResult::Finished = thread.raise(signal) {
                break;
            }
        }
    }

    pub fn add_child(
        &self,
        child: &Arc<Process>,
        procs: ProofMut<'_, ProcessList>,
    ) {
        assert!(self.all_procs_link.is_linked(), "Dead process");
        self.children.access_mut(procs).insert(child.clone());
    }

    pub fn add_thread(
        &self,
        thread: &Arc<Thread>,
        procs: ProofMut<'_, ProcessList>,
    ) {
        assert!(self.all_procs_link.is_linked(), "Dead process");
        self.threads.access_mut(procs).insert(thread.clone());
    }

    pub async fn wait(
        &self,
        wait_id: WaitId,
        no_block: bool,
        trace_stop: bool,
        trace_continue: bool,
    ) -> KResult<Option<WaitObject>> {
        let wait_object = {
            let mut unlocked_waits = None;

            loop {
                let mut waits = match unlocked_waits {
                    Some(wait) => wait.await?,
                    None => {
                        self.wait_list
                            .entry(wait_id, trace_stop, trace_continue)
                            .await
                    }
                };

                if let Some(object) = waits.get() {
                    break object;
                }

                if self.children.access(waits.process_list.prove()).is_empty() {
                    return Err(ECHILD);
                }

                if no_block {
                    return Ok(None);
                }

                unlocked_waits = Some(waits.wait(no_block));
            }
        };

        if wait_object.stopped().is_some() || wait_object.is_continue() {
            Ok(Some(wait_object))
        } else {
            let mut procs = ProcessList::get().write().await;
            procs.remove_process(wait_object.pid);
            assert!(self
                .children
                .access_mut(procs.prove_mut())
                .find_mut(&wait_object.pid)
                .remove()
                .is_some());

            Ok(Some(wait_object))
        }
    }

    /// Create a new session for the process.
    pub async fn setsid(self: &Arc<Self>) -> KResult<u32> {
        let mut process_list = ProcessList::get().write().await;
        // If there exists a session that has the same sid as our pid, we can't create a new
        // session. The standard says that we should create a new process group and be the
        // only process in the new process group and session.
        if process_list.try_find_session(self.pid).is_some() {
            return Err(EPERM);
        }

        self.pgroup(process_list.prove())
            .remove_member(self, &mut process_list);

        let session = Session::new(self, &mut process_list);
        let pgroup = ProcessGroup::new(self, &session, &mut process_list);

        let old_session =
            unsafe { self.session.swap(Some(session.clone())) }.unwrap();
        let old_pgroup =
            unsafe { self.pgroup.swap(Some(pgroup.clone())) }.unwrap();

        call_rcu(move || {
            drop(old_session);
            drop(old_pgroup);
        });

        Ok(pgroup.pgid)
    }

    /// Set the process group id of the process to `pgid`.
    ///
    /// This function does the actual work.
    fn do_setpgid(
        self: &Arc<Self>,
        pgid: u32,
        procs: &mut ProcessList,
    ) -> KResult<()> {
        // SAFETY: We are holding the process list lock.
        let session = unsafe { self.session.load_locked().unwrap() };

        // Changing the process group of a session leader is not allowed.
        if session.sid == self.pid {
            return Err(EPERM);
        }

        let cur_pgroup = self.pgroup(procs.prove()).clone();
        let existing_pgroup = procs.try_find_pgroup(pgid);

        if let Some(new_pgroup) = &existing_pgroup {
            // Move us to an existing process group.
            // Check that the two groups are in the same session.
            if new_pgroup.session.sid != session.sid {
                return Err(EPERM);
            }

            // If we are already in the process group, we are done.
            if new_pgroup.pgid == cur_pgroup.pgid {
                return Ok(());
            }
        } else {
            // Create a new process group only if `pgid` matches our `pid`.
            if pgid != self.pid {
                return Err(EPERM);
            }
        }

        // Permission checks done. Let's do the actual work.
        cur_pgroup.remove_member(self, procs);

        let new_pgroup;
        if let Some(pgroup) = existing_pgroup {
            pgroup.add_member(self, procs.prove_mut());
            new_pgroup = pgroup;
        } else {
            new_pgroup = ProcessGroup::new(self, &session, procs);
        }

        unsafe {
            // SAFETY: `cur_pgroup` held above.
            self.pgroup.swap(Some(new_pgroup));
        }
        call_rcu(move || drop(cur_pgroup));

        Ok(())
    }

    /// Set the process group id of the process `pid` to `pgid`.
    ///
    /// This function should be called on the process that issued the syscall in order to do
    /// permission checks.
    pub async fn setpgid(self: &Arc<Self>, pid: u32, pgid: u32) -> KResult<()> {
        let mut procs = ProcessList::get().write().await;
        // We may set pgid of either the calling process or a child process.
        if pid == self.pid {
            self.do_setpgid(pgid, &mut procs)
        } else {
            let child = {
                // If `pid` refers to one of our children, the thread leaders must be
                // in out children list.
                let children = self.children.access(procs.prove());
                let child = children.find(&pid).clone_pointer().ok_or(ESRCH)?;

                // Changing the process group of a child is only allowed
                // if we are in the same session.
                if child.session(procs.prove()).sid
                    != self.session(procs.prove()).sid
                {
                    return Err(EPERM);
                }

                child
            };

            // TODO: Check whether we, as a child, have already performed an `execve`.
            //       If so, we should return `Err(EACCES)`.
            child.do_setpgid(pgid, &mut procs)
        }
    }

    /// Provide locked (consistent) access to the session.
    pub fn session<'r>(
        &'r self,
        _procs: Proof<'r, ProcessList>,
    ) -> BorrowedArc<'r, Session> {
        // SAFETY: We are holding the process list lock.
        unsafe { self.session.load_locked() }.unwrap()
    }

    /// Provide locked (consistent) access to the process group.
    pub fn pgroup<'r>(
        &'r self,
        _procs: Proof<'r, ProcessList>,
    ) -> BorrowedArc<'r, ProcessGroup> {
        // SAFETY: We are holding the process list lock.
        unsafe { self.pgroup.load_locked() }.unwrap()
    }

    /// Provide locked (consistent) access to the parent process.
    pub fn parent<'r>(
        &'r self,
        _procs: Proof<'r, ProcessList>,
    ) -> BorrowedArc<'r, Process> {
        // SAFETY: We are holding the process list lock.
        unsafe { self.parent.load_locked() }.unwrap()
    }

    /// Provide RCU locked (maybe inconsistent) access to the session.
    pub fn session_rcu(&self) -> RCUReadGuard<'_, BorrowedArc<'_, Session>> {
        self.session.load().unwrap()
    }

    /// Provide RCU locked (maybe inconsistent) access to the process group.
    pub fn pgroup_rcu(
        &self,
    ) -> RCUReadGuard<'_, BorrowedArc<'_, ProcessGroup>> {
        self.pgroup.load().unwrap()
    }

    /// Provide RCU locked (maybe inconsistent) access to the parent process.
    pub fn parent_rcu(
        &self,
    ) -> Option<RCUReadGuard<'_, BorrowedArc<'_, Process>>> {
        self.parent.load()
    }

    pub fn notify(
        &self,
        signal: Option<Signal>,
        wait: WaitObject,
        procs: Proof<'_, ProcessList>,
    ) {
        self.wait_list.notify(wait);

        if let Some(signal) = signal {
            // If we have a signal, we raise it to the process.
            self.raise(signal, procs);
        }
    }

    pub fn notify_batch(&self) -> NotifyBatch<'_, '_, '_> {
        NotifyBatch {
            wait_procs: self.wait_list.wait_procs.lock(),
            process: self,
            cv: &self.wait_list.cv_wait_procs,
            needs_notify: false,
        }
    }
}

impl WaitList {
    pub fn new() -> Self {
        Self {
            wait_procs: Spin::new(VecDeque::new()),
            cv_wait_procs: CondVar::new(),
        }
    }

    fn notify(&self, wait: WaitObject) {
        let mut wait_procs = self.wait_procs.lock();
        wait_procs.push_back(wait);
        self.cv_wait_procs.notify_all();
    }

    pub fn drain_exited(&self) -> DrainExited<'_> {
        DrainExited {
            wait_procs: self.wait_procs.lock(),
        }
    }

    /// # Safety
    /// Locks `ProcessList` and `WaitList` at the same time. When `wait` is called,
    /// releases the lock on `ProcessList` and `WaitList` and waits on `cv_wait_procs`.
    pub async fn entry(
        &self,
        wait_id: WaitId,
        want_stop: bool,
        want_continue: bool,
    ) -> Entry<'_, '_, '_> {
        Entry {
            process_list: ProcessList::get().read().await,
            wait_procs: self.wait_procs.lock(),
            cv: &self.cv_wait_procs,
            want_stop,
            want_continue,
            want_id: wait_id,
        }
    }
}

impl Entry<'_, '_, '_> {
    pub fn get(&mut self) -> Option<WaitObject> {
        if let Some(idx) = self
            .wait_procs
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if item.stopped().is_some() {
                    self.want_stop
                } else if item.is_continue() {
                    self.want_continue
                } else {
                    true
                }
            })
            .filter(|(_, item)| match self.want_id {
                WaitId::Any => true,
                WaitId::Pid(pid) => item.pid == pid,
                WaitId::Pgid(pgid) => {
                    if let Some(process) =
                        self.process_list.try_find_process(item.pid)
                    {
                        return process.pgroup(self.process_list.prove()).pgid
                            == pgid;
                    }
                    false
                }
            })
            .map(|(idx, _)| idx)
            .next()
        {
            Some(self.wait_procs.remove(idx).unwrap())
        } else {
            None
        }
    }

    pub fn wait(
        self,
        no_block: bool,
    ) -> impl core::future::Future<Output = KResult<Self>> + Send {
        let wait_procs = self.wait_procs.unlock();

        async move {
            let process_list = self.cv.wait(self.process_list).await;
            let wait_procs = wait_procs.relock().await;

            if !no_block && Thread::current().signal_list.has_pending_signal() {
                Err(EINTR)
            } else {
                Ok(Self {
                    wait_procs,
                    process_list,
                    cv: self.cv,
                    want_stop: self.want_stop,
                    want_continue: self.want_continue,
                    want_id: self.want_id,
                })
            }
        }
    }
}

impl DrainExited<'_> {
    pub fn into_iter(&mut self) -> impl Iterator<Item = WaitObject> + '_ {
        // We don't propagate stop and continue to the new parent.
        self.wait_procs
            .drain(..)
            .filter(|item| item.stopped().is_none() && !item.is_continue())
    }
}

impl NotifyBatch<'_, '_, '_> {
    pub fn notify(&mut self, wait: WaitObject) {
        self.needs_notify = true;
        self.wait_procs.push_back(wait);
    }

    /// Finish the batch and notify all if we have notified some processes.
    pub fn finish(mut self, procs: Proof<'_, ProcessList>) {
        if self.needs_notify {
            self.cv.notify_all();
            self.process.raise(Signal::SIGCHLD, procs);
            self.needs_notify = false;
        }
    }
}

impl Drop for NotifyBatch<'_, '_, '_> {
    fn drop(&mut self) {
        if self.needs_notify {
            panic!("NotifyBatch dropped without calling finish");
        }
    }
}

pub fn alloc_pid() -> u32 {
    static NEXT_PID: AtomicU32 = AtomicU32::new(1);
    NEXT_PID.fetch_add(1, Ordering::Relaxed)
}
