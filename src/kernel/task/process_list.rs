use alloc::{
    collections::btree_map::BTreeMap,
    sync::{Arc, Weak},
};
use bindings::KERNEL_PML4;

use crate::{
    prelude::*,
    rcu::rcu_sync,
    sync::{preempt, AsRefMutPosition as _, AsRefPosition as _},
};

use lazy_static::lazy_static;

use super::{Process, ProcessGroup, Scheduler, Session, Signal, Thread, WaitObject, WaitType};

pub struct ProcessList {
    /// The init process.
    init: Option<Arc<Process>>,
    /// The kernel idle process.
    idle: Option<Arc<Process>>,
    /// All threads except the idle thread.
    threads: BTreeMap<u32, Arc<Thread>>,
    /// All processes except the idle process.
    processes: BTreeMap<u32, Weak<Process>>,
    /// All process groups.
    pgroups: BTreeMap<u32, Weak<ProcessGroup>>,
    /// All sessions.
    sessions: BTreeMap<u32, Weak<Session>>,
}

lazy_static! {
    static ref GLOBAL_PROC_LIST: RwSemaphore<ProcessList> = {
        RwSemaphore::new(ProcessList {
            init: None,
            idle: None,
            threads: BTreeMap::new(),
            processes: BTreeMap::new(),
            pgroups: BTreeMap::new(),
            sessions: BTreeMap::new(),
        })
    };
}

impl ProcessList {
    pub fn get() -> &'static RwSemaphore<Self> {
        &GLOBAL_PROC_LIST
    }

    pub fn add_session(&mut self, session: &Arc<Session>) {
        self.sessions.insert(session.sid, Arc::downgrade(session));
    }

    pub fn add_pgroup(&mut self, pgroup: &Arc<ProcessGroup>) {
        self.pgroups.insert(pgroup.pgid, Arc::downgrade(pgroup));
    }

    pub fn add_process(&mut self, process: &Arc<Process>) {
        self.processes.insert(process.pid, Arc::downgrade(process));
    }

    pub fn add_thread(&mut self, thread: &Arc<Thread>) {
        self.threads.insert(thread.tid, thread.clone());
    }

    pub fn kill_current(signal: Signal) -> ! {
        ProcessList::get()
            .lock()
            .do_kill_process(&Thread::current().process, WaitType::Signaled(signal));
        Scheduler::schedule_noreturn()
    }

    pub fn remove_process(&mut self, pid: u32) {
        // Thread group leader has the same tid as the pid.
        if let Some(thread) = self.threads.remove(&pid) {
            self.processes.remove(&pid);

            // SAFETY: We wait until all references are dropped below with `rcu_sync()`.
            let session = unsafe { thread.process.session.swap(None) }.unwrap();
            let pgroup = unsafe { thread.process.pgroup.swap(None) }.unwrap();
            let _parent = unsafe { thread.process.parent.swap(None) }.unwrap();
            pgroup.remove_member(pid, self.as_pos_mut());
            rcu_sync();

            if Arc::strong_count(&pgroup) == 1 {
                self.pgroups.remove(&pgroup.pgid);
            }

            if Arc::strong_count(&session) == 1 {
                self.sessions.remove(&session.sid);
            }
        } else {
            panic!("Process {} not found", pid);
        }
    }

    pub fn init_process(&self) -> &Arc<Process> {
        self.init.as_ref().unwrap()
    }

    pub fn idle_process(&self) -> &Arc<Process> {
        self.idle.as_ref().unwrap()
    }

    pub fn try_find_thread(&self, tid: u32) -> Option<&Arc<Thread>> {
        self.threads.get(&tid)
    }

    pub fn try_find_process(&self, pid: u32) -> Option<Arc<Process>> {
        self.processes.get(&pid).and_then(Weak::upgrade)
    }

    pub fn try_find_pgroup(&self, pgid: u32) -> Option<Arc<ProcessGroup>> {
        self.pgroups.get(&pgid).and_then(Weak::upgrade)
    }

    pub fn try_find_session(&self, sid: u32) -> Option<Arc<Session>> {
        self.sessions.get(&sid).and_then(Weak::upgrade)
    }

    /// Make the process a zombie and notify the parent.
    pub fn do_kill_process(&mut self, process: &Arc<Process>, status: WaitType) {
        if self.idle_process().pid == process.pid {
            panic!("idle exited");
        }

        if self.init_process().pid == process.pid {
            panic!("init exited");
        }

        preempt::disable();

        let inner = process.inner.access_mut(self.as_pos_mut());
        // TODO!!!!!!: When we are killing multiple threads, we need to wait until all
        // the threads are stopped then proceed.
        for thread in inner.threads.values().map(|t| t.upgrade().unwrap()) {
            assert!(thread.tid == Thread::current().tid);
            Scheduler::get().lock().set_zombie(&thread);
            thread.files.close_all();
        }

        // If we are the session leader, we should drop the control terminal.
        if process.session(self.as_pos()).sid == process.pid {
            if let Some(terminal) = process.session(self.as_pos()).drop_control_terminal() {
                terminal.drop_session();
            }
        }

        // Release the MMList as well as the page table.
        // Before we release the page table, we need to switch to the kernel page table.
        arch::set_root_page_table(KERNEL_PML4 as usize);
        unsafe {
            process.mm_list.release();
        }

        // Make children orphans (adopted by init)
        {
            let init = self.init_process();
            inner.children.retain(|_, child| {
                let child = child.upgrade().unwrap();
                // SAFETY: `child.parent` must be ourself. So we don't need to free it.
                unsafe { child.parent.swap(Some(init.clone())) };
                init.add_child(&child, self.as_pos_mut());

                false
            });
        }

        let mut init_notify = self.init_process().notify_batch();
        process
            .wait_list
            .drain_exited()
            .into_iter()
            .for_each(|item| init_notify.notify(item));
        init_notify.finish(self.as_pos());

        process.parent(self.as_pos()).notify(
            WaitObject {
                pid: process.pid,
                code: status,
            },
            self.as_pos(),
        );

        preempt::enable();
    }
}

pub unsafe fn init_multitasking(init_fn: unsafe extern "C" fn()) {
    let mut procs = ProcessList::get().lock();

    let init_process = Process::new_for_init(1, procs.as_mut());
    let init_thread = Thread::new_for_init(
        Arc::from(b"[kernel kinit]".as_slice()),
        1,
        &init_process,
        procs.as_mut(),
    );

    let init_session = Session::new(procs.as_mut(), &init_process);
    let init_pgroup = init_session.new_group(procs.as_mut(), &init_process);

    assert!(init_process.session.swap(Some(init_session)).is_none());
    assert!(init_process.pgroup.swap(Some(init_pgroup)).is_none());

    let idle_process = Process::new_for_init(0, procs.as_mut());
    let idle_thread = Thread::new_for_init(
        Arc::from(b"[kernel idle#BS]".as_slice()),
        0,
        &idle_process,
        procs.as_mut(),
    );

    procs.init = Some(init_process);
    procs.idle = Some(idle_process);

    let mut scheduler = Scheduler::get().lock_irq();

    init_thread.init(init_fn as usize);
    scheduler.uwake(&init_thread);
    Scheduler::set_idle_and_current(idle_thread);
}
