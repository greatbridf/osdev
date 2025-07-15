use super::{
    process_group::ProcessGroupBuilder, signal::RaiseResult, thread::ThreadBuilder, ProcessGroup,
    ProcessList, Session, Thread,
};
use crate::kernel::constants::{ECHILD, EINTR, EINVAL, EPERM, ESRCH};
use crate::kernel::task::{CloneArgs, CloneFlags};
use crate::{
    kernel::mem::MMList,
    prelude::*,
    rcu::{rcu_sync, RCUPointer, RCUReadGuard},
    sync::CondVar,
};
use alloc::{
    collections::{btree_map::BTreeMap, vec_deque::VecDeque},
    sync::{Arc, Weak},
};
use core::sync::atomic::{AtomicU32, Ordering};
use eonix_mm::address::VAddr;
use eonix_runtime::task::Task;
use eonix_sync::{
    AsProof as _, AsProofMut as _, Locked, Proof, ProofMut, RwLockReadGuard, SpinGuard,
    UnlockableGuard as _, UnlockedGuard as _,
};
use pointers::BorrowedArc;
use posix_types::constants::{
    CLD_CONTINUED, CLD_DUMPED, CLD_EXITED, CLD_KILLED, CLD_STOPPED, P_PGID, P_PIDFD,
};
use posix_types::constants::{P_ALL, P_PID};
use posix_types::signal::Signal;
use posix_types::SIGNAL_COREDUMP;

pub struct ProcessBuilder {
    mm_list: Option<MMList>,
    exit_signal: Option<Signal>,
    parent: Option<Arc<Process>>,
    thread_builder: Option<ThreadBuilder>,
    pgroup: Option<Arc<ProcessGroup>>,
    session: Option<Arc<Session>>,
    pid: Option<u32>,
}

#[derive(Debug)]
pub struct Process {
    /// Process id
    ///
    /// This should never change during the life of the process.
    pub pid: u32,

    pub wait_list: WaitList,
    pub mm_list: MMList,

    pub exit_signal: Option<Signal>,

    pub shm_areas: Spin<BTreeMap<VAddr, usize>>,

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

    /// All things related to the process list.
    pub(super) inner: Locked<ProcessInner, ProcessList>,
}

#[derive(Debug)]
pub(super) struct ProcessInner {
    pub(super) children: BTreeMap<u32, Weak<Process>>,
    pub(super) threads: BTreeMap<u32, Weak<Thread>>,
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
            P_PIDFD => {
                panic!("PDIFD type is unsupported")
            }
            _ => Err(EINVAL),
        }
    }

    pub fn from_id(id: i32, thread: &Thread) -> Self {
        if id < -1 {
            WaitId::Pgid((-id).cast_unsigned())
        } else if id == -1 {
            WaitId::Any
        } else if id == 0 {
            let procs = Task::block_on(ProcessList::get().read());
            WaitId::Pgid(thread.process.pgroup(procs.prove()).pgid)
        } else {
            WaitId::Pid(id.cast_unsigned())
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
            WaitType::Signaled(signal @ SIGNAL_COREDUMP!()) => signal.into_raw() | 0x80,
            WaitType::Signaled(signal) => signal.into_raw(),
            WaitType::Stopped(signal) => 0x7f | (signal.into_raw() << 8),
            WaitType::Continued => 0xffff,
        }
    }

    pub fn to_status_code(self) -> (u32, u32) {
        // TODO: CLD_TRAPPED
        match self {
            WaitType::Exited(status) => (status, CLD_EXITED),
            WaitType::Signaled(signal @ SIGNAL_COREDUMP!()) => (signal.into_raw(), CLD_DUMPED),
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

    pub fn clone_from(mut self, process: Arc<Process>, clone_args: &CloneArgs) -> Self {
        let mm_list = if clone_args.flags.contains(CloneFlags::CLONE_VM) {
            Task::block_on(process.mm_list.new_shared())
        } else {
            Task::block_on(process.mm_list.new_cloned())
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

    pub fn build(self, process_list: &mut ProcessList) -> (Arc<Thread>, Arc<Process>) {
        let mm_list = self.mm_list.unwrap_or_else(|| MMList::new());

        let process = Arc::new(Process {
            pid: self.pid.expect("should set pid before building"),
            wait_list: WaitList::new(),
            mm_list,
            shm_areas: Spin::new(BTreeMap::new()),
            exit_signal: self.exit_signal,
            parent: RCUPointer::empty(),
            pgroup: RCUPointer::empty(),
            session: RCUPointer::empty(),
            inner: Locked::new(
                ProcessInner {
                    children: BTreeMap::new(),
                    threads: BTreeMap::new(),
                },
                process_list,
            ),
        });

        process_list.add_process(&process);

        let thread_builder = self.thread_builder.expect("Thread builder is not set");
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
            None => ProcessGroupBuilder::new()
                .leader(&process)
                .session(session.clone())
                .build(process_list),
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
        let inner = self.inner.access(procs);
        for thread in inner.threads.values().map(|t| t.upgrade().unwrap()) {
            if let RaiseResult::Finished = thread.raise(signal) {
                break;
            }
        }
    }

    pub(super) fn add_child(&self, child: &Arc<Process>, procs: ProofMut<'_, ProcessList>) {
        assert!(self
            .inner
            .access_mut(procs)
            .children
            .insert(child.pid, Arc::downgrade(child))
            .is_none());
    }

    pub(super) fn add_thread(&self, thread: &Arc<Thread>, procs: ProofMut<'_, ProcessList>) {
        assert!(self
            .inner
            .access_mut(procs)
            .threads
            .insert(thread.tid, Arc::downgrade(thread))
            .is_none());
    }

    pub async fn wait(
        &self,
        wait_id: WaitId,
        no_block: bool,
        trace_stop: bool,
        trace_continue: bool,
    ) -> KResult<Option<WaitObject>> {
        let wait_object = {
            let mut waits = self.wait_list.entry(wait_id, trace_stop, trace_continue);
            loop {
                if let Some(object) = waits.get() {
                    break object;
                }

                if self
                    .inner
                    .access(waits.process_list.prove())
                    .children
                    .is_empty()
                {
                    return Err(ECHILD);
                }

                if no_block {
                    return Ok(None);
                }

                waits = waits.wait(no_block).await?;
            }
        };

        if wait_object.stopped().is_some() || wait_object.is_continue() {
            Ok(Some(wait_object))
        } else {
            let mut procs = ProcessList::get().write().await;
            procs.remove_process(wait_object.pid);
            assert!(self
                .inner
                .access_mut(procs.prove_mut())
                .children
                .remove(&wait_object.pid)
                .is_some());

            Ok(Some(wait_object))
        }
    }

    /// Create a new session for the process.
    pub fn setsid(self: &Arc<Self>) -> KResult<u32> {
        let mut process_list = Task::block_on(ProcessList::get().write());
        // If there exists a session that has the same sid as our pid, we can't create a new
        // session. The standard says that we should create a new process group and be the
        // only process in the new process group and session.
        if process_list.try_find_session(self.pid).is_some() {
            return Err(EPERM);
        }
        let session = Session::new(self, &mut process_list);
        let pgroup = ProcessGroupBuilder::new()
            .leader(self)
            .session(session.clone())
            .build(&mut process_list);

        {
            let _old_session = unsafe { self.session.swap(Some(session.clone())) }.unwrap();
            let old_pgroup = unsafe { self.pgroup.swap(Some(pgroup.clone())) }.unwrap();
            old_pgroup.remove_member(self.pid, process_list.prove_mut());
            Task::block_on(rcu_sync());
        }

        Ok(pgroup.pgid)
    }

    /// Set the process group id of the process to `pgid`.
    ///
    /// This function does the actual work.
    fn do_setpgid(self: &Arc<Self>, pgid: u32, procs: &mut ProcessList) -> KResult<()> {
        // SAFETY: We are holding the process list lock.
        let session = unsafe { self.session.load_locked().unwrap() };
        let pgroup = unsafe { self.pgroup.load_locked().unwrap() };

        // Changing the process group of a session leader is not allowed.
        if session.sid == self.pid {
            return Err(EPERM);
        }

        let new_pgroup = if let Some(new_pgroup) = procs.try_find_pgroup(pgid) {
            // Move us to an existing process group.
            // Check that the two groups are in the same session.
            if new_pgroup.session.upgrade().unwrap().sid != session.sid {
                return Err(EPERM);
            }

            // If we are already in the process group, we are done.
            if new_pgroup.pgid == pgroup.pgid {
                return Ok(());
            }

            new_pgroup.add_member(self, procs.prove_mut());

            new_pgroup
        } else {
            // Create a new process group only if `pgid` matches our `pid`.
            if pgid != self.pid {
                return Err(EPERM);
            }

            ProcessGroupBuilder::new()
                .leader(self)
                .session(session.clone())
                .build(procs)
        };

        pgroup.remove_member(self.pid, procs.prove_mut());
        {
            let _old_pgroup = unsafe { self.pgroup.swap(Some(new_pgroup)) }.unwrap();
            Task::block_on(rcu_sync());
        }

        Ok(())
    }

    /// Set the process group id of the process `pid` to `pgid`.
    ///
    /// This function should be called on the process that issued the syscall in order to do
    /// permission checks.
    pub fn setpgid(self: &Arc<Self>, pid: u32, pgid: u32) -> KResult<()> {
        let mut procs = Task::block_on(ProcessList::get().write());
        // We may set pgid of either the calling process or a child process.
        if pid == self.pid {
            self.do_setpgid(pgid, &mut procs)
        } else {
            let child = {
                // If `pid` refers to one of our children, the thread leaders must be
                // in out children list.
                let children = &self.inner.access(procs.prove()).children;
                let child = {
                    let child = children.get(&pid);
                    child.and_then(Weak::upgrade).ok_or(ESRCH)?
                };

                // Changing the process group of a child is only allowed
                // if we are in the same session.
                if child.session(procs.prove()).sid != self.session(procs.prove()).sid {
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
    pub fn session<'r>(&'r self, _procs: Proof<'r, ProcessList>) -> BorrowedArc<'r, Session> {
        // SAFETY: We are holding the process list lock.
        unsafe { self.session.load_locked() }.unwrap()
    }

    /// Provide locked (consistent) access to the process group.
    pub fn pgroup<'r>(&'r self, _procs: Proof<'r, ProcessList>) -> BorrowedArc<'r, ProcessGroup> {
        // SAFETY: We are holding the process list lock.
        unsafe { self.pgroup.load_locked() }.unwrap()
    }

    /// Provide locked (consistent) access to the parent process.
    pub fn parent<'r>(&'r self, _procs: Proof<'r, ProcessList>) -> BorrowedArc<'r, Process> {
        // SAFETY: We are holding the process list lock.
        unsafe { self.parent.load_locked() }.unwrap()
    }

    /// Provide RCU locked (maybe inconsistent) access to the session.
    pub fn session_rcu(&self) -> RCUReadGuard<'_, BorrowedArc<Session>> {
        self.session.load().unwrap()
    }

    /// Provide RCU locked (maybe inconsistent) access to the process group.
    pub fn pgroup_rcu(&self) -> RCUReadGuard<'_, BorrowedArc<ProcessGroup>> {
        self.pgroup.load().unwrap()
    }

    /// Provide RCU locked (maybe inconsistent) access to the parent process.
    pub fn parent_rcu(&self) -> Option<RCUReadGuard<'_, BorrowedArc<Process>>> {
        self.parent.load()
    }

    pub fn notify(&self, signal: Option<Signal>, wait: WaitObject, procs: Proof<'_, ProcessList>) {
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

    pub fn drain_exited(&self) -> DrainExited {
        DrainExited {
            wait_procs: self.wait_procs.lock(),
        }
    }

    /// # Safety
    /// Locks `ProcessList` and `WaitList` at the same time. When `wait` is called,
    /// releases the lock on `ProcessList` and `WaitList` and waits on `cv_wait_procs`.
    pub fn entry(&self, wait_id: WaitId, want_stop: bool, want_continue: bool) -> Entry {
        Entry {
            process_list: Task::block_on(ProcessList::get().read()),
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
                    let procs = Task::block_on(ProcessList::get().read());
                    if let Some(process) = procs.try_find_process(item.pid) {
                        return process.pgroup(procs.prove()).pgid == pgid;
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

    pub fn wait(self, no_block: bool) -> impl core::future::Future<Output = KResult<Self>> {
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
