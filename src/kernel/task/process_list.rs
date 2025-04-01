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

use super::{Process, ProcessGroup, Session, Signal, Thread, WaitObject, WaitType};

pub struct ProcessList {
    /// The init process.
    init: Option<Arc<Process>>,
    /// All threads.
    threads: BTreeMap<u32, Arc<Thread>>,
    /// All processes.
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
        unsafe {
            let mut process_list = ProcessList::get().lock();
            preempt::disable();

            // SAFETY: Preemption disabled.
            process_list.do_kill_process(&Thread::current().process, WaitType::Signaled(signal));
        }

        unsafe {
            // SAFETY: Preempt count == 1.
            Thread::runnable().exit();
        }
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

    pub fn set_init_process(&mut self, init: Arc<Process>) {
        let old_init = self.init.replace(init);
        assert!(old_init.is_none(), "Init process already set");
    }

    pub fn init_process(&self) -> &Arc<Process> {
        self.init.as_ref().unwrap()
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
    /// # Safety
    /// This function needs to be called with preemption disabled.
    pub unsafe fn do_kill_process(&mut self, process: &Arc<Process>, status: WaitType) {
        if process.pid == 1 {
            panic!("init exited");
        }

        let inner = process.inner.access_mut(self.as_pos_mut());
        // TODO!!!!!!: When we are killing multiple threads, we need to wait until all
        // the threads are stopped then proceed.
        for thread in inner.threads.values().map(|t| t.upgrade().unwrap()) {
            assert!(thread.tid == Thread::current().tid);
            // TODO: Send SIGKILL to all threads.
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
    }
}
