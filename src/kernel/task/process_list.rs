use core::sync::atomic::Ordering;

use super::{Process, ProcessGroup, Session, Thread, WaitObject, WaitType};
use crate::{
    kernel::{task::futex_wake, user::UserPointerMut},
    rcu::rcu_sync,
};
use alloc::{
    collections::btree_map::BTreeMap,
    sync::{Arc, Weak},
};
use eonix_mm::address::Addr;
use eonix_sync::{AsProof as _, AsProofMut as _, RwLock};

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

static GLOBAL_PROC_LIST: RwLock<ProcessList> = RwLock::new(ProcessList {
    init: None,
    threads: BTreeMap::new(),
    processes: BTreeMap::new(),
    pgroups: BTreeMap::new(),
    sessions: BTreeMap::new(),
});

impl ProcessList {
    pub fn get() -> &'static RwLock<Self> {
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

    pub async fn remove_process(&mut self, pid: u32) {
        // Thread group leader has the same tid as the pid.
        if let Some(thread) = self.threads.remove(&pid) {
            self.processes.remove(&pid);

            // SAFETY: We wait until all references are dropped below with `rcu_sync()`.
            let session = unsafe { thread.process.session.swap(None) }.unwrap();
            let pgroup = unsafe { thread.process.pgroup.swap(None) }.unwrap();
            let _parent = unsafe { thread.process.parent.swap(None) }.unwrap();
            pgroup.remove_member(pid, self.prove_mut());
            rcu_sync().await;

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
    /// This function will destroy the process and all its threads.
    /// It is the caller's responsibility to ensure that the process is not
    /// running or will not run after this function is called.
    pub async unsafe fn do_exit(
        &mut self,
        thread: &Thread,
        exit_status: WaitType,
        is_exiting_group: bool,
    ) {
        let process = thread.process.clone();

        if process.pid == 1 {
            panic!("init exited");
        }

        let inner = process.inner.access_mut(self.prove_mut());

        thread.dead.store(true, Ordering::SeqCst);

        if is_exiting_group {
            // TODO: Send SIGKILL to all threads.
            // todo!()
        }

        if thread.tid != process.pid {
            self.threads.remove(&thread.tid);
            inner.threads.remove(&thread.tid).unwrap();
        }

        if let Some(clear_ctid) = thread.get_clear_ctid() {
            let _ = UserPointerMut::new(clear_ctid).unwrap().write(0u32);

            let _ = futex_wake(clear_ctid.addr(), None, 1).await;
        }

        if let Some(robust_list) = thread.get_robust_list() {
            let _ = robust_list.wake_all().await;
        }

        // main thread exit
        if thread.tid == process.pid {
            assert_eq!(thread.tid, process.pid);

            thread.files.close_all();

            // If we are the session leader, we should drop the control terminal.
            if process.session(self.prove()).sid == process.pid {
                if let Some(terminal) = process.session(self.prove()).drop_control_terminal().await
                {
                    terminal.drop_session().await;
                }
            }

            // Release the MMList as well as the page table.
            unsafe {
                // SAFETY: We are exiting the process, so no one might be using it.
                process.mm_list.replace(None);
            }

            // Make children orphans (adopted by init)
            {
                let init = self.init_process();
                inner.children.retain(|_, child| {
                    let child = child.upgrade().unwrap();
                    // SAFETY: `child.parent` must be ourself. So we don't need to free it.
                    unsafe { child.parent.swap(Some(init.clone())) };
                    init.add_child(&child, self.prove_mut());

                    false
                });
            }

            let mut init_notify = self.init_process().notify_batch();
            process
                .wait_list
                .drain_exited()
                .into_iter()
                .for_each(|item| init_notify.notify(item));
            init_notify.finish(self.prove());

            process.parent(self.prove()).notify(
                process.exit_signal,
                WaitObject {
                    pid: process.pid,
                    code: exit_status,
                },
                self.prove(),
            );
        }
    }
}
