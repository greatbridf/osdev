use core::{
    arch::asm,
    cell::RefCell,
    cmp,
    sync::atomic::{self, AtomicU32},
};

use crate::{
    kernel::{
        mem::{
            phys::{CachedPP, PhysPtr},
            MMList,
        },
        terminal::Terminal,
        user::dataflow::CheckedUserPointer,
        vfs::FsContext,
    },
    prelude::*,
    sync::{preempt, CondVar},
};

use alloc::{
    collections::{btree_map::BTreeMap, vec_deque::VecDeque},
    sync::{Arc, Weak},
};
use bindings::{ECHILD, EINTR, EINVAL, EPERM, ESRCH};
use lazy_static::lazy_static;

use crate::kernel::vfs::filearray::FileArray;

use super::{
    signal::{RaiseResult, Signal, SignalList},
    KernelStack, Scheduler,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Preparing,
    Running,
    Ready,
    Zombie,
    ISleep,
    USleep,
}

#[derive(Debug, Clone, Copy)]
pub struct WaitObject {
    pub pid: u32,
    pub code: u32,
}

impl WaitObject {
    pub fn is_stopped(&self) -> bool {
        self.code & 0x7f == 0x7f
    }

    pub fn is_continue(&self) -> bool {
        self.code == 0xffff
    }
}

#[derive(Debug)]
struct SessionInner {
    /// Foreground process group
    foreground: Weak<ProcessGroup>,
    control_terminal: Option<Arc<Terminal>>,
    groups: BTreeMap<u32, Weak<ProcessGroup>>,
}

#[derive(Debug)]
pub struct Session {
    sid: u32,
    leader: Weak<Process>,

    inner: Spin<SessionInner>,
}

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

    /// Thread TLS descriptor 32-bit
    tls_desc32: u64,

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
    static ref GLOBAL_PROC_LIST: ProcessList = {
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
        ProcessList::get().do_kill_process(
            &Thread::current().process,
            ((signal.to_signum() + 128) << 8) | (signal.to_signum() & 0xff),
        );

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
    pub fn do_kill_process(&self, process: &Arc<Process>, status: u32) {
        if &self.init == process {
            panic!("init exited");
        }

        preempt::disable();

        let mut inner = process.inner.lock();
        // TODO!!!!!!: When we are killing multiple threads, we need to wait until all
        // the threads are stopped then proceed.
        for thread in inner.threads.values().map(|t| t.upgrade().unwrap()) {
            assert!(&thread == Thread::current());
            Scheduler::get().lock().set_zombie(&thread);
            thread.files.close_all();
        }

        // Unmap all user memory areas
        process.mm_list.clear_user();

        // Make children orphans (adopted by init)
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

        let has_waiting = {
            let mut init_waits = self.init.wait_list.wait_procs.lock();
            let mut waits = process.wait_list.wait_procs.lock();

            let mut done_some_work = false;
            waits.retain(|item| {
                if !item.is_stopped() && !item.is_continue() {
                    init_waits.push_back(*item);
                    done_some_work = true;
                }
                false
            });

            done_some_work
        };

        if has_waiting {
            self.init.wait_list.cv_wait_procs.notify_all();
        }

        {
            {
                let parent_wait_list = &inner.parent.as_ref().unwrap().wait_list;
                let mut parent_waits = parent_wait_list.wait_procs.lock();
                parent_waits.push_back(WaitObject {
                    pid: process.pid,
                    code: status,
                });
                parent_wait_list.cv_wait_procs.notify_all();
            }
        }

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

        let process = Arc::new(Self {
            pid: Self::alloc_pid(),
            wait_list: WaitList::new(),
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

    fn new_for_init(pid: u32, parent: Option<Arc<Self>>) -> Arc<Self> {
        let process = Arc::new_cyclic(|weak| {
            let session = Session::new(pid, weak.clone());
            let pgroup = ProcessGroup::new_for_init(pid, weak.clone(), Arc::downgrade(&session));

            session.add_member(&pgroup);
            Self {
                pid,
                wait_list: WaitList::new(),
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
        let mut wait_list = self.wait_list.wait_procs.lock();
        let wait_object = loop {
            if let Some(idx) = wait_list
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    if item.is_stopped() {
                        trace_stop
                    } else if item.is_continue() {
                        trace_continue
                    } else {
                        true
                    }
                })
                .map(|(idx, _)| idx)
                .next()
            {
                break wait_list.remove(idx).unwrap();
            }

            if self.inner.lock().children.is_empty() {
                return Err(ECHILD);
            }
            if no_block {
                return Ok(None);
            }
            self.wait_list.cv_wait_procs.wait(&mut wait_list);
            if Thread::current().signal_list.has_pending_signal() {
                return Err(EINTR);
            }
        };

        if wait_object.is_stopped() || wait_object.is_continue() {
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

            inner.session = Session::new(self.pid, Arc::downgrade(self));
            ProcessList::get().add_session(&inner.session);

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
    fn new_for_init(name: Arc<[u8]>, process: &Arc<Process>) -> Arc<Self> {
        let thread = Arc::new(Self {
            tid: process.pid,
            process: process.clone(),
            files: FileArray::new_for_init(),
            fs_context: FsContext::new_for_init(),
            signal_list: SignalList::new(),
            kstack: RefCell::new(KernelStack::new()),
            state: Spin::new(ThreadState::Preparing),
            inner: Spin::new(ThreadInner {
                name,
                tls_desc32: 0,
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
            state: Spin::new(ThreadState::Preparing),
            inner: Spin::new(ThreadInner {
                name: other_inner.name.clone(),
                tls_desc32: other_inner.tls_desc32,
                set_child_tid: other_inner.set_child_tid,
            }),
        });

        ProcessList::get().add_thread(&thread);
        other.process.add_child(&thread);
        process.add_thread(&thread);
        thread
    }

    pub fn current<'lt>() -> &'lt Arc<Self> {
        Scheduler::current()
    }

    pub fn do_stop(self: &Arc<Self>) {
        if let Some(parent) = self.process.parent() {
            parent.wait_list.notify_stop(self.process.pid);
        }

        preempt::disable();

        // `SIGSTOP` can only be waken up by `SIGCONT` or `SIGKILL`.
        // SAFETY: Preempt disabled above.
        Scheduler::get().lock().usleep(self);
        Scheduler::schedule();
    }

    pub fn do_continue(self: &Arc<Self>) {
        if let Some(parent) = self.process.parent() {
            parent.wait_list.notify_continue(self.process.pid);
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

    pub fn load_thread_area32(&self) {
        let inner = self.inner.lock();
        if inner.tls_desc32 == 0 {
            return;
        }

        // SAFETY: `tls32` should be per cpu.
        let tls32_addr = CachedPP::new(0x0 + 7 * 8);
        tls32_addr.as_mut::<u64>().clone_from(&inner.tls_desc32);

        unsafe {
            asm!(
                "mov %gs, %ax",
                "mov %ax, %gs",
                out("ax") _,
                options(att_syntax)
            )
        };
    }

    pub fn set_thread_area(&self, desc: &mut UserDescriptor) -> KResult<()> {
        let mut inner = self.inner.lock();

        // Clear the TLS area if it is not present.
        if desc.flags.is_read_exec_only() && !desc.flags.is_present() {
            if desc.limit != 0 && desc.base != 0 {
                CheckedUserPointer::new(desc.base as _, desc.limit as _)?.zero()?;
            }
            return Ok(());
        }

        if desc.entry != u32::MAX || !desc.flags.is_32bit_segment() {
            return Err(EINVAL);
        }
        desc.entry = 7;

        inner.tls_desc32 = desc.limit as u64 & 0xffff;
        inner.tls_desc32 |= (desc.base as u64 & 0xffffff) << 16;
        inner.tls_desc32 |= 0x4_0_f2_000000_0000;
        inner.tls_desc32 |= (desc.limit as u64 & 0xf_0000) << (48 - 16);

        if desc.flags.is_limit_in_pages() {
            inner.tls_desc32 |= 1 << 55;
        }

        inner.tls_desc32 |= (desc.base as u64 & 0xff_000000) << (56 - 24);

        Ok(())
    }

    /// This function is used to prepare the kernel stack for the thread in `Preparing` state.
    ///
    /// # Safety
    /// Calling this function on a thread that is not in `Preparing` state will panic.
    pub fn prepare_kernel_stack<F: FnOnce(&mut KernelStack)>(&self, func: F) {
        let mut state = self.state.lock();
        assert!(matches!(*state, ThreadState::Preparing));

        // SAFETY: We are in the preparing state with `state` locked.
        func(&mut self.kstack.borrow_mut());

        // Enter USleep state. Await for the thread to be scheduled manually.
        *state = ThreadState::USleep;
    }

    pub fn load_interrupt_stack(&self) {
        self.kstack.borrow().load_interrupt_stack();
    }

    /// Get a pointer to `self.sp` so we can use it in `context_switch()`.
    ///
    /// # Safety
    /// Save the pointer somewhere or pass it to a function that will use it is UB.
    pub unsafe fn get_sp_ptr(&self) -> *mut usize {
        self.kstack.borrow().get_sp_ptr()
    }

    pub fn set_name(&self, name: Arc<[u8]>) {
        self.inner.lock().name = name;
    }

    pub fn get_name(&self) -> Arc<[u8]> {
        self.inner.lock().name.clone()
    }
}

// TODO: Maybe we can find a better way instead of using `RefCell` for `KernelStack`?
unsafe impl Sync for Thread {}

impl WaitList {
    pub fn new() -> Self {
        Self {
            wait_procs: Spin::new(VecDeque::new()),
            cv_wait_procs: CondVar::new(),
        }
    }

    pub fn notify_continue(&self, pid: u32) {
        let mut wait_procs = self.wait_procs.lock();
        wait_procs.push_back(WaitObject { pid, code: 0xffff });
        self.cv_wait_procs.notify_all();
    }

    pub fn notify_stop(&self, pid: u32) {
        let mut wait_procs = self.wait_procs.lock();
        wait_procs.push_back(WaitObject { pid, code: 0x7f });
        self.cv_wait_procs.notify_all();
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

    Thread::current().load_interrupt_stack();
    Thread::current().process.mm_list.switch_page_table();
}
