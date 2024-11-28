use core::{
    arch::naked_asm,
    cell::{RefCell, UnsafeCell},
    cmp,
    sync::atomic::{self, AtomicBool, AtomicU32},
};

use crate::{
    kernel::{
        cpu::current_cpu, mem::MMList, terminal::Terminal, user::dataflow::CheckedUserPointer,
        vfs::FsContext,
    },
    prelude::*,
    sync::{preempt, CondVar, SpinGuard},
};

use alloc::{
    collections::{btree_map::BTreeMap, vec_deque::VecDeque},
    sync::{Arc, Weak},
};
use bindings::{ECHILD, EINTR, EPERM, ESRCH};
use lazy_static::lazy_static;

use crate::kernel::vfs::filearray::FileArray;

use super::{
    signal::{RaiseResult, Signal, SignalList},
    KernelStack, Scheduler,
};

use arch::{InterruptContext, TaskContext, UserTLS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Preparing,
    Running,
    Ready,
    Zombie,
    ISleep,
    USleep,
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
            WaitType::Signaled(signal) if signal.is_coredump() => signal.to_signum() | 0x80,
            WaitType::Signaled(signal) => signal.to_signum(),
            WaitType::Stopped(signal) => 0x7f | (signal.to_signum() << 8),
            WaitType::Continued => 0xffff,
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

#[derive(Debug)]
struct SessionInner {
    /// Foreground process group
    foreground: Weak<ProcessGroup>,
    control_terminal: Option<Arc<Terminal>>,
    groups: BTreeMap<u32, Weak<ProcessGroup>>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Session {
    sid: u32,
    leader: Weak<Process>,

    inner: Spin<SessionInner>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ProcessGroup {
    pgid: u32,
    leader: Weak<Process>,
    session: Weak<Session>,

    processes: Spin<BTreeMap<u32, Weak<Process>>>,
}

#[derive(Debug)]
struct ProcessInner {
    /// Parent process
    ///
    /// Parent process must be valid during the whole life of the process.
    /// The only case that parent process may be `None` is when this is the init process
    /// or the process is kernel thread.
    parent: Option<Arc<Process>>,

    /// Process group
    pgroup: Arc<ProcessGroup>,

    /// Session
    session: Arc<Session>,

    /// Children list
    children: BTreeMap<u32, Weak<Thread>>,

    /// Thread list
    threads: BTreeMap<u32, Weak<Thread>>,
}

#[derive(Debug)]
pub struct WaitList {
    wait_procs: Spin<VecDeque<WaitObject>>,
    cv_wait_procs: CondVar,
    process: Weak<Process>,
}

pub struct NotifyBatch<'waitlist, 'cv, 'process> {
    wait_procs: SpinGuard<'waitlist, VecDeque<WaitObject>>,
    cv: &'cv CondVar,
    process: &'process Weak<Process>,
    needs_notify: bool,
}

pub struct Entry<'waitlist, 'cv> {
    wait_procs: SpinGuard<'waitlist, VecDeque<WaitObject>>,
    cv: &'cv CondVar,
    want_stop: bool,
    want_continue: bool,
}

pub struct DrainExited<'waitlist> {
    wait_procs: SpinGuard<'waitlist, VecDeque<WaitObject>>,
}

#[derive(Debug)]
pub struct Process {
    /// Process id
    ///
    /// This should never change during the life of the process.
    pub pid: u32,

    pub wait_list: WaitList,
    pub mm_list: Arc<MMList>,
    inner: Spin<ProcessInner>,
}

impl PartialOrd for Process {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.pid.partial_cmp(&other.pid)
    }
}

impl Ord for Process {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.pid.cmp(&other.pid)
    }
}

impl PartialEq for Process {
    fn eq(&self, other: &Self) -> bool {
        self.pid == other.pid
    }
}

impl Eq for Process {}

#[derive(Debug)]
struct ThreadInner {
    /// Thread name
    name: Arc<[u8]>,

    /// Thread TLS
    tls: Option<UserTLS>,

    /// User pointer
    /// Store child thread's tid when child thread returns to user space.
    set_child_tid: usize,
}

pub struct Thread {
    pub tid: u32,
    pub process: Arc<Process>,

    pub files: Arc<FileArray>,
    pub fs_context: Arc<FsContext>,

    pub signal_list: SignalList,

    /// Thread state for scheduler use.
    pub state: Spin<ThreadState>,

    pub oncpu: AtomicBool,

    /// Thread context
    pub context: UnsafeCell<TaskContext>,

    /// Kernel stack
    /// Never access this directly.
    ///
    /// We can only touch kernel stack when the process is neither running nor sleeping.
    /// AKA, the process is in the ready queue and will return to `schedule` context.
    kstack: RefCell<KernelStack>,

    inner: Spin<ThreadInner>,
}

impl PartialOrd for Thread {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.tid.partial_cmp(&other.tid)
    }
}

impl Ord for Thread {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.tid.cmp(&other.tid)
    }
}

impl PartialEq for Thread {
    fn eq(&self, other: &Self) -> bool {
        self.tid == other.tid
    }
}

impl Eq for Thread {}

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct UserDescriptorFlags(u32);

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UserDescriptor {
    entry: u32,
    base: u32,
    limit: u32,
    flags: UserDescriptorFlags,
}

pub struct ProcessList {
    init: Arc<Process>,
    threads: Spin<BTreeMap<u32, Arc<Thread>>>,
    processes: Spin<BTreeMap<u32, Weak<Process>>>,
    pgroups: Spin<BTreeMap<u32, Weak<ProcessGroup>>>,
    sessions: Spin<BTreeMap<u32, Weak<Session>>>,
}

impl Session {
    fn new(sid: u32, leader: Weak<Process>) -> Arc<Self> {
        Arc::new(Self {
            sid,
            leader,
            inner: Spin::new(SessionInner {
                foreground: Weak::new(),
                control_terminal: None,
                groups: BTreeMap::new(),
            }),
        })
    }

    fn add_member(&self, pgroup: &Arc<ProcessGroup>) {
        self.inner
            .lock()
            .groups
            .insert(pgroup.pgid, Arc::downgrade(pgroup));
    }

    pub fn foreground_pgid(&self) -> Option<u32> {
        self.inner.lock().foreground.upgrade().map(|fg| fg.pgid)
    }

    /// Set the foreground process group.
    pub fn set_foreground_pgid(&self, pgid: u32) -> KResult<()> {
        let mut inner = self.inner.lock();
        let group = inner.groups.get(&pgid);

        if let Some(group) = group {
            inner.foreground = group.clone();
            Ok(())
        } else {
            // TODO!!!: Check if the process group is valid.
            //          We assume that the process group is valid for now.
            Err(EPERM)
        }
    }

    /// Only session leaders can set the control terminal.
    /// Make sure we've checked that before calling this function.
    pub fn set_control_terminal(
        self: &Arc<Self>,
        terminal: &Arc<Terminal>,
        forced: bool,
    ) -> KResult<()> {
        let mut inner = self.inner.lock();
        if let Some(_) = inner.control_terminal.as_ref() {
            if let Some(session) = terminal.session().as_ref() {
                if session.sid == self.sid {
                    return Ok(());
                }
            }
            return Err(EPERM);
        }
        terminal.set_session(self, forced)?;
        inner.control_terminal = Some(terminal.clone());
        inner.foreground = Arc::downgrade(&Thread::current().process.pgroup());
        Ok(())
    }

    /// Drop the control terminal reference inside the session.
    /// DO NOT TOUCH THE TERMINAL'S SESSION FIELD.
    pub fn drop_control_terminal(&self) -> Option<Arc<Terminal>> {
        let mut inner = self.inner.lock();
        inner.foreground = Weak::new();
        inner.control_terminal.take()
    }

    pub fn raise_foreground(&self, signal: Signal) {
        if let Some(fg) = self.inner.lock().foreground.upgrade() {
            fg.raise(signal);
        }
    }
}

impl ProcessGroup {
    fn new_for_init(pgid: u32, leader: Weak<Process>, session: Weak<Session>) -> Arc<Self> {
        Arc::new(Self {
            pgid,
            leader: leader.clone(),
            session,
            processes: Spin::new(BTreeMap::from([(pgid, leader)])),
        })
    }

    fn new(leader: &Arc<Process>, session: &Arc<Session>) -> Arc<Self> {
        let pgroup = Arc::new(Self {
            pgid: leader.pid,
            leader: Arc::downgrade(leader),
            session: Arc::downgrade(session),
            processes: Spin::new(BTreeMap::from([(leader.pid, Arc::downgrade(leader))])),
        });

        session.add_member(&pgroup);
        pgroup
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        let mut process = self.process.inner.lock();

        process.threads.remove(&self.tid);
        if let Some(parent) = &process.parent {
            parent.inner.lock().children.remove(&self.tid);
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let inner = self.inner.lock();
        assert!(inner.children.is_empty());

        inner.pgroup.processes.lock().remove(&self.pid);
        ProcessList::get().processes.lock().remove(&self.pid);
    }
}

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        if let Some(session) = self.session.upgrade() {
            session.inner.lock().groups.remove(&self.pgid);
        }
    }
}

lazy_static! {
    static ref GLOBAL_PROC_LIST: ProcessList = unsafe {
        let init_process = Process::new_for_init(1, None);
        let init_thread = Thread::new_for_init(b"[kernel kinit]".as_slice().into(), &init_process);
        Scheduler::set_current(init_thread.clone());

        let idle_process = Process::new_for_init(0, None);
        let idle_thread =
            Thread::new_for_init(b"[kernel idle#BS]".as_slice().into(), &idle_process);
        Scheduler::set_idle(idle_thread.clone());

        let init_session_weak = Arc::downgrade(&init_process.inner.lock().session);
        let init_pgroup_weak = Arc::downgrade(&init_process.inner.lock().pgroup);

        ProcessList {
            sessions: Spin::new(BTreeMap::from([(1, init_session_weak)])),
            pgroups: Spin::new(BTreeMap::from([(1, init_pgroup_weak)])),
            threads: Spin::new(BTreeMap::from([
                (1, init_thread.clone()),
                (0, idle_thread.clone()),
            ])),
            processes: Spin::new(BTreeMap::from([
                (1, Arc::downgrade(&init_process)),
                (0, Arc::downgrade(&idle_process)),
            ])),
            init: init_process,
        }
    };
}
impl ProcessList {
    pub fn get() -> &'static Self {
        &GLOBAL_PROC_LIST
    }

    pub fn add_session(&self, session: &Arc<Session>) {
        self.sessions
            .lock()
            .insert(session.sid, Arc::downgrade(session));
    }

    pub fn add_pgroup(&self, pgroup: &Arc<ProcessGroup>) {
        self.pgroups
            .lock()
            .insert(pgroup.pgid, Arc::downgrade(pgroup));
    }

    pub fn add_process(&self, process: &Arc<Process>) {
        self.processes
            .lock()
            .insert(process.pid, Arc::downgrade(process));
    }

    pub fn add_thread(&self, thread: &Arc<Thread>) {
        self.threads.lock().insert(thread.tid, thread.clone());
    }

    pub fn kill_current(signal: Signal) -> ! {
        ProcessList::get().do_kill_process(&Thread::current().process, WaitType::Signaled(signal));
        Scheduler::schedule_noreturn()
    }

    // TODO!!!!!!: Reconsider this
    fn remove(&self, tid: u32) {
        if let None = self.threads.lock().remove(&tid) {
            panic!("Thread {} not found", tid);
        }
    }

    pub fn try_find_process(&self, pid: u32) -> Option<Arc<Process>> {
        self.processes.lock().get(&pid).and_then(Weak::upgrade)
    }

    pub fn try_find_thread(&self, tid: u32) -> Option<Arc<Thread>> {
        self.threads.lock().get(&tid).cloned()
    }

    pub fn try_find_pgroup(&self, pgid: u32) -> Option<Arc<ProcessGroup>> {
        self.pgroups.lock().get(&pgid).and_then(Weak::upgrade)
    }

    pub fn try_find_session(&self, sid: u32) -> Option<Arc<Session>> {
        self.sessions.lock().get(&sid).and_then(Weak::upgrade)
    }

    /// Make the process a zombie and notify the parent.
    pub fn do_kill_process(&self, process: &Arc<Process>, status: WaitType) {
        if &self.init == process {
            panic!("init exited");
        }

        preempt::disable();

        let mut inner = process.inner.lock();
        // TODO!!!!!!: When we are killing multiple threads, we need to wait until all
        // the threads are stopped then proceed.
        for thread in inner.threads.values().map(|t| t.upgrade().unwrap()) {
            assert!(&thread == Thread::current().as_ref());
            Scheduler::get().lock().set_zombie(&thread);
            thread.files.close_all();
        }

        // If we are the session leader, we should drop the control terminal.
        if inner.session.sid == process.pid {
            if let Some(terminal) = inner.session.drop_control_terminal() {
                terminal.drop_session();
            }
        }

        // Unmap all user memory areas
        process.mm_list.clear_user();

        // Make children orphans (adopted by init)
        {
            let mut init_inner = self.init.inner.lock();

            inner.children.retain(|_, child| {
                let child = child.upgrade().unwrap();
                let mut child_inner = child.process.inner.lock();
                if child_inner.parent.as_ref().unwrap() == &self.init {
                    return false;
                }

                child_inner.parent = Some(self.init.clone());
                init_inner.add_child(&child);

                false
            });
        }

        let mut init_notify = self.init.wait_list.notify_batch();
        process
            .wait_list
            .drain_exited()
            .into_iter()
            .for_each(|item| init_notify.notify(item));
        init_notify.finish();

        inner.parent.as_ref().unwrap().wait_list.notify(WaitObject {
            pid: process.pid,
            code: status,
        });

        preempt::enable();
    }
}

impl ProcessGroup {
    fn add_member(&self, process: &Arc<Process>) {
        self.processes
            .lock()
            .insert(process.pid, Arc::downgrade(process));
    }

    fn remove_member(&self, pid: u32) {
        self.processes.lock().remove(&pid);
    }

    pub fn raise(&self, signal: Signal) {
        let processes = self.processes.lock();
        for process in processes.values().map(|p| p.upgrade().unwrap()) {
            process.raise(signal);
        }
    }
}

impl ProcessInner {
    fn add_child(&mut self, child: &Arc<Thread>) {
        self.children.insert(child.tid, Arc::downgrade(child));
    }

    fn add_thread(&mut self, thread: &Arc<Thread>) {
        self.threads.insert(thread.tid, Arc::downgrade(thread));
    }
}

/// PID 0 and 1 is created manually so we start from 2.
static NEXT_PID: AtomicU32 = AtomicU32::new(2);
impl Process {
    fn alloc_pid() -> u32 {
        NEXT_PID.fetch_add(1, atomic::Ordering::Relaxed)
    }

    pub fn new_cloned(other: &Arc<Self>) -> Arc<Self> {
        let other_inner = other.inner.lock();

        let process = Arc::new_cyclic(|weak| Self {
            pid: Self::alloc_pid(),
            wait_list: WaitList::new(weak.clone()),
            mm_list: MMList::new_cloned(&other.mm_list),
            inner: Spin::new(ProcessInner {
                pgroup: other_inner.pgroup.clone(),
                session: other_inner.session.clone(),
                children: BTreeMap::new(),
                threads: BTreeMap::new(),
                parent: Some(other.clone()),
            }),
        });

        ProcessList::get().add_process(&process);
        other_inner.pgroup.add_member(&process);
        process
    }

    unsafe fn new_for_init(pid: u32, parent: Option<Arc<Self>>) -> Arc<Self> {
        let process = Arc::new_cyclic(|weak| {
            let session = Session::new(pid, weak.clone());
            let pgroup = ProcessGroup::new_for_init(pid, weak.clone(), Arc::downgrade(&session));

            session.add_member(&pgroup);
            Self {
                pid,
                wait_list: WaitList::new(weak.clone()),
                mm_list: MMList::new(),
                inner: Spin::new(ProcessInner {
                    parent,
                    pgroup,
                    session,
                    children: BTreeMap::new(),
                    threads: BTreeMap::new(),
                }),
            }
        });

        process.inner.lock().pgroup.add_member(&process);
        process
    }

    pub fn raise(&self, signal: Signal) {
        let inner = self.inner.lock();
        for thread in inner.threads.values().map(|t| t.upgrade().unwrap()) {
            if let RaiseResult::Finished = thread.raise(signal) {
                break;
            }
        }
    }

    fn add_child(&self, child: &Arc<Thread>) {
        self.inner.lock().add_child(child);
    }

    fn add_thread(&self, thread: &Arc<Thread>) {
        self.inner.lock().add_thread(thread);
    }

    pub fn wait(
        &self,
        no_block: bool,
        trace_stop: bool,
        trace_continue: bool,
    ) -> KResult<Option<WaitObject>> {
        let mut waits = self.wait_list.entry(trace_stop, trace_continue);
        let wait_object = loop {
            if let Some(object) = waits.get() {
                break object;
            }

            if self.inner.lock().children.is_empty() {
                return Err(ECHILD);
            }

            if no_block {
                return Ok(None);
            }

            waits.wait()?;
        };

        if wait_object.stopped().is_some() || wait_object.is_continue() {
            Ok(Some(wait_object))
        } else {
            ProcessList::get().remove(wait_object.pid);
            Ok(Some(wait_object))
        }
    }

    /// Create a new session for the process.
    pub fn setsid(self: &Arc<Self>) -> KResult<u32> {
        let mut inner = self.inner.lock();
        // If there exists a session that has the same sid as our pid, we can't create a new
        // session. The standard says that we should create a new process group and be the
        // only process in the new process group and session.
        if ProcessList::get().try_find_session(self.pid).is_some() {
            return Err(EPERM);
        }
        inner.session = Session::new(self.pid, Arc::downgrade(self));
        ProcessList::get().add_session(&inner.session);

        inner.pgroup.remove_member(self.pid);
        inner.pgroup = ProcessGroup::new(self, &inner.session);
        ProcessList::get().add_pgroup(&inner.pgroup);

        Ok(inner.pgroup.pgid)
    }

    /// Set the process group id of the process to `pgid`.
    ///
    /// This function does the actual work.
    fn do_setpgid(self: &Arc<Self>, pgid: u32) -> KResult<()> {
        let mut inner = self.inner.lock();

        // Changing the process group of a session leader is not allowed.
        if inner.session.sid == self.pid {
            return Err(EPERM);
        }

        // Move us to an existing process group.
        if let Some(pgroup) = ProcessList::get().try_find_pgroup(pgid) {
            // Move the process to a process group in a different session in not allowed.
            if pgroup.session.upgrade().unwrap().sid != inner.session.sid {
                return Err(EPERM);
            }

            // If we are already in the process group, we are done.
            if pgroup.pgid == inner.pgroup.pgid {
                return Ok(());
            }

            inner.pgroup.remove_member(self.pid);
            inner.pgroup = pgroup;
        } else {
            // Create a new process group only if `pgid` matches our `pid`.
            if pgid != self.pid {
                return Err(EPERM);
            }

            inner.pgroup.remove_member(self.pid);
            inner.pgroup = ProcessGroup::new(self, &inner.session);
            ProcessList::get().add_pgroup(&inner.pgroup);
        }

        Ok(())
    }

    /// Set the process group id of the process `pid` to `pgid`.
    ///
    /// This function should be called on the process that issued the syscall in order to do
    /// permission checks.
    pub fn setpgid(self: &Arc<Self>, pid: u32, pgid: u32) -> KResult<()> {
        // We may set pgid of either the calling process or a child process.
        if pid == self.pid {
            self.do_setpgid(pgid)
        } else {
            let child = {
                // If `pid` refers to one of our children, the thread leaders must be
                // in out children list.
                let inner = self.inner.lock();
                let child = {
                    let child = inner.children.get(&pid);
                    child.and_then(Weak::upgrade).ok_or(ESRCH)?
                };

                // Changing the process group of a child is only allowed
                // if we are in the same session.
                if child.process.sid() != inner.session.sid {
                    return Err(EPERM);
                }

                child
            };

            // TODO: Check whether we, as a child, have already performed an `execve`.
            //       If so, we should return `Err(EACCES)`.
            child.process.do_setpgid(pgid)
        }
    }

    pub fn sid(&self) -> u32 {
        self.inner.lock().session.sid
    }

    pub fn pgid(&self) -> u32 {
        self.inner.lock().pgroup.pgid
    }

    pub fn session(&self) -> Arc<Session> {
        self.inner.lock().session.clone()
    }

    pub fn pgroup(&self) -> Arc<ProcessGroup> {
        self.inner.lock().pgroup.clone()
    }
}

#[allow(dead_code)]
impl UserDescriptorFlags {
    fn is_32bit_segment(&self) -> bool {
        self.0 & 0b1 != 0
    }

    fn contents(&self) -> u32 {
        self.0 & 0b110
    }

    fn is_read_exec_only(&self) -> bool {
        self.0 & 0b1000 != 0
    }

    fn is_limit_in_pages(&self) -> bool {
        self.0 & 0b10000 != 0
    }

    fn is_present(&self) -> bool {
        self.0 & 0b100000 == 0
    }

    fn is_usable(&self) -> bool {
        self.0 & 0b1000000 != 0
    }
}

impl Thread {
    pub unsafe fn new_for_init(name: Arc<[u8]>, process: &Arc<Process>) -> Arc<Self> {
        let thread = Arc::new(Self {
            tid: process.pid,
            process: process.clone(),
            files: FileArray::new_for_init(),
            fs_context: FsContext::new_for_init(),
            signal_list: SignalList::new(),
            kstack: RefCell::new(KernelStack::new()),
            context: UnsafeCell::new(TaskContext::new()),
            state: Spin::new(ThreadState::Preparing),
            oncpu: AtomicBool::new(false),
            inner: Spin::new(ThreadInner {
                name,
                tls: None,
                set_child_tid: 0,
            }),
        });

        process.add_thread(&thread);
        thread
    }

    pub fn new_cloned(other: &Self) -> Arc<Self> {
        let process = Process::new_cloned(&other.process);

        let other_state = other.state.lock();
        let other_inner = other.inner.lock();
        assert!(matches!(*other_state, ThreadState::Running));

        let signal_list = other.signal_list.clone();
        signal_list.clear_pending();

        let thread = Arc::new(Self {
            tid: process.pid,
            process: process.clone(),
            files: FileArray::new_cloned(&other.files),
            fs_context: FsContext::new_cloned(&other.fs_context),
            signal_list,
            kstack: RefCell::new(KernelStack::new()),
            context: UnsafeCell::new(TaskContext::new()),
            state: Spin::new(ThreadState::Preparing),
            oncpu: AtomicBool::new(false),
            inner: Spin::new(ThreadInner {
                name: other_inner.name.clone(),
                tls: other_inner.tls.clone(),
                set_child_tid: other_inner.set_child_tid,
            }),
        });

        ProcessList::get().add_thread(&thread);
        other.process.add_child(&thread);
        process.add_thread(&thread);
        thread
    }

    pub fn current<'lt>() -> BorrowedArc<'lt, Self> {
        Scheduler::current()
    }

    pub fn do_stop(self: &Arc<Self>, signal: Signal) {
        if let Some(parent) = self.process.parent() {
            parent.wait_list.notify(WaitObject {
                pid: self.process.pid,
                code: WaitType::Stopped(signal),
            });
        }

        preempt::disable();

        // `SIGSTOP` can only be waken up by `SIGCONT` or `SIGKILL`.
        // SAFETY: Preempt disabled above.
        Scheduler::get().lock().usleep(self);
        Scheduler::schedule();
    }

    pub fn do_continue(self: &Arc<Self>) {
        if let Some(parent) = self.process.parent() {
            parent.wait_list.notify(WaitObject {
                pid: self.process.pid,
                code: WaitType::Continued,
            });
        }
    }

    pub fn raise(self: &Arc<Thread>, signal: Signal) -> RaiseResult {
        match self.signal_list.raise(signal) {
            RaiseResult::ShouldIWakeUp => {
                Scheduler::get().lock_irq().iwake(self);
                RaiseResult::Finished
            }
            RaiseResult::ShouldUWakeUp => {
                Scheduler::get().lock_irq().uwake(self);
                RaiseResult::Finished
            }
            result => result,
        }
    }

    /// # Safety
    /// This function is unsafe because it accesses the `current_cpu()`, which needs
    /// to be called in a preemption disabled context.
    pub unsafe fn load_thread_area32(&self) {
        if let Some(tls) = self.inner.lock().tls.as_ref() {
            // SAFETY: Preemption is disabled.
            tls.load(current_cpu());
        }
    }

    pub fn set_thread_area(&self, desc: &mut UserDescriptor) -> KResult<()> {
        let mut inner = self.inner.lock();

        // Clear the TLS area if it is not present.
        if desc.flags.is_read_exec_only() && !desc.flags.is_present() {
            if desc.limit == 0 || desc.base == 0 {
                return Ok(());
            }

            let len = if desc.flags.is_limit_in_pages() {
                (desc.limit as usize) << 12
            } else {
                desc.limit as usize
            };

            CheckedUserPointer::new(desc.base as _, len)?.zero()?;
            return Ok(());
        }

        let (tls, entry) = UserTLS::new32(desc.base, desc.limit, desc.flags.is_limit_in_pages());
        desc.entry = entry;
        inner.tls = Some(tls);
        Ok(())
    }

    pub fn fork_init(&self, interrupt_context: InterruptContext) {
        let mut state = self.state.lock();
        *state = ThreadState::USleep;

        let sp = self.kstack.borrow().init(interrupt_context);
        unsafe {
            self.get_context_mut_ptr()
                .as_mut()
                .unwrap()
                .init(fork_return as usize, sp);
        }
    }

    pub fn init(&self, entry: usize) {
        let mut state = self.state.lock();
        *state = ThreadState::USleep;
        unsafe {
            self.get_context_mut_ptr()
                .as_mut()
                .unwrap()
                .init(entry, self.get_kstack_bottom());
        }
    }

    /// # Safety
    /// This function is unsafe because it accesses the `current_cpu()`, which needs
    /// to be called in a preemption disabled context.
    pub unsafe fn load_interrupt_stack(&self) {
        self.kstack.borrow().load_interrupt_stack();
    }

    pub fn get_kstack_bottom(&self) -> usize {
        self.kstack.borrow().get_stack_bottom()
    }

    pub unsafe fn get_context_mut_ptr(&self) -> *mut TaskContext {
        self.context.get()
    }

    pub fn set_name(&self, name: Arc<[u8]>) {
        self.inner.lock().name = name;
    }

    pub fn get_name(&self) -> Arc<[u8]> {
        self.inner.lock().name.clone()
    }
}

#[naked]
unsafe extern "C" fn fork_return() {
    // We don't land on the typical `Scheduler::schedule()` function, so we need to
    // manually enable preemption.
    naked_asm! {
        "
        call {preempt_enable}
        swapgs
        pop %rax
        pop %rbx
        pop %rcx
        pop %rdx
        pop %rdi
        pop %rsi
        pop %r8
        pop %r9
        pop %r10
        pop %r11
        pop %r12
        pop %r13
        pop %r14
        pop %r15
        pop %rbp
        add $16, %rsp
        iretq
        ",
        preempt_enable = sym preempt::enable,
        options(att_syntax),
    }
}

// TODO: Maybe we can find a better way instead of using `RefCell` for `KernelStack`?
unsafe impl Sync for Thread {}

impl WaitList {
    pub fn new(process: Weak<Process>) -> Self {
        Self {
            wait_procs: Spin::new(VecDeque::new()),
            cv_wait_procs: CondVar::new(),
            process,
        }
    }

    pub fn notify(&self, wait: WaitObject) {
        let mut wait_procs = self.wait_procs.lock();
        wait_procs.push_back(wait);
        self.cv_wait_procs.notify_all();

        self.process
            .upgrade()
            .expect("`process` must be valid if we are using `WaitList`")
            .raise(Signal::SIGCHLD);
    }

    /// Notify some processes in batch. The process is waken up if we have really notified
    /// some processes.
    ///
    /// # Lock
    /// This function locks the `wait_procs` and returns a `NotifyBatch` that
    /// will unlock it on dropped.
    pub fn notify_batch(&self) -> NotifyBatch {
        NotifyBatch {
            wait_procs: self.wait_procs.lock(),
            cv: &self.cv_wait_procs,
            needs_notify: false,
            process: &self.process,
        }
    }

    pub fn drain_exited(&self) -> DrainExited {
        DrainExited {
            wait_procs: self.wait_procs.lock(),
        }
    }

    pub fn entry(&self, want_stop: bool, want_continue: bool) -> Entry {
        Entry {
            wait_procs: self.wait_procs.lock(),
            cv: &self.cv_wait_procs,
            want_stop,
            want_continue,
        }
    }
}

impl Entry<'_, '_> {
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
            .map(|(idx, _)| idx)
            .next()
        {
            Some(self.wait_procs.remove(idx).unwrap())
        } else {
            None
        }
    }

    pub fn wait(&mut self) -> KResult<()> {
        self.cv.wait(&mut self.wait_procs);
        if Thread::current().signal_list.has_pending_signal() {
            return Err(EINTR);
        }
        Ok(())
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
        self.wait_procs.push_back(wait);
    }

    /// Finish the batch and notify all if we have notified some processes.
    pub fn finish(self) {}
}

impl Drop for NotifyBatch<'_, '_, '_> {
    fn drop(&mut self) {
        if self.needs_notify {
            self.cv.notify_all();

            self.process
                .upgrade()
                .expect("`process` must be valid if we are using `WaitList`")
                .raise(Signal::SIGCHLD);
        }
    }
}

impl Process {
    pub fn parent(&self) -> Option<Arc<Process>> {
        self.inner.lock().parent.clone()
    }
}

pub fn init_multitasking() {
    // Lazy init
    assert!(ProcessList::get().try_find_thread(1).is_some());

    unsafe {
        // SAFETY: Preemption is disabled outside this function.
        Thread::current().load_interrupt_stack();
    }
    Thread::current().process.mm_list.switch_page_table();
}
