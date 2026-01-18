use alloc::collections::btree_map::BTreeMap;
use alloc::collections::vec_deque::VecDeque;
use alloc::sync::{Arc, Weak};
use core::pin::pin;
use core::sync::atomic::Ordering;

use eonix_runtime::scheduler::RUNTIME;
use eonix_sync::{AsProof as _, AsProofMut as _, RwLock, Spin, WaitList};

use super::loader::LoadInfo;
use super::{
    alloc_pid, Process, ProcessBuilder, ProcessGroup, Session, Thread, ThreadBuilder, WaitObject,
};
use crate::rcu::rcu_sync;

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

    fn set_init_process(&mut self, init: Arc<Process>) {
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

    pub async fn sys_init(load_info: LoadInfo) {
        let thread_builder = ThreadBuilder::new()
            .name(Arc::from(&b"busybox"[..]))
            .entry(load_info.entry_ip, load_info.sp);

        let mut process_list = ProcessList::get().write().await;
        let (thread, process) = ProcessBuilder::new()
            .pid(alloc_pid())
            .mm_list(load_info.mm_list)
            .thread_builder(thread_builder)
            .build(&mut process_list);

        process_list.set_init_process(process);

        // TODO!!!: Remove this.
        thread.files.open_console();

        RUNTIME.spawn(Reaper::daemon());
        RUNTIME.spawn(thread.run());
    }

    pub fn send_to_reaper(thread: Arc<Thread>) {
        GLOBAL_REAPER.reap_list.lock().push_back(thread);
        GLOBAL_REAPER.wait.notify_one();
    }
}

struct Reaper {
    reap_list: Spin<VecDeque<Arc<Thread>>>,
    wait: WaitList,
}

static GLOBAL_REAPER: Reaper = Reaper {
    reap_list: Spin::new(VecDeque::new()),
    wait: WaitList::new(),
};

impl Reaper {
    async fn reap(&self, thread: Arc<Thread>) {
        let exit_status = thread
            .exit_status
            .lock()
            .take()
            .expect("Exited thread with no exit status");

        let process = &thread.process;

        if process.pid == 1 && thread.tid == process.pid {
            panic!("init exited");
        }

        let mut procs = ProcessList::get().write().await;

        let inner = process.inner.access_mut(procs.prove_mut());

        thread.dead.store(true, Ordering::SeqCst);

        if thread.tid != process.pid {
            procs.threads.remove(&thread.tid);
            inner.threads.remove(&thread.tid).unwrap();
        }

        // main thread exit
        if thread.tid == process.pid {
            assert_eq!(thread.tid, process.pid);

            thread.files.close_all().await;

            // If we are the session leader, we should drop the control terminal.
            if process.session(procs.prove()).sid == process.pid {
                if let Some(terminal) = process.session(procs.prove()).drop_control_terminal().await
                {
                    terminal.drop_session().await;
                }
            }

            // Release the MMList as well as the page table.
            process.mm_list.release();

            // Make children orphans (adopted by init)
            {
                let init = procs.init_process();
                inner.children.retain(|_, child| {
                    let child = child.upgrade().unwrap();
                    // SAFETY: `child.parent` must be ourself. So we don't need to free it.
                    unsafe { child.parent.swap(Some(init.clone())) };
                    init.add_child(&child, procs.prove_mut());

                    false
                });
            }

            let mut init_notify = procs.init_process().notify_batch();
            process
                .wait_list
                .drain_exited()
                .into_iter()
                .for_each(|item| init_notify.notify(item));
            init_notify.finish(procs.prove());

            process.parent(procs.prove()).notify(
                process.exit_signal,
                WaitObject {
                    pid: process.pid,
                    code: exit_status,
                },
                procs.prove(),
            );
        }
    }

    async fn daemon() {
        let me = &GLOBAL_REAPER;

        loop {
            let mut wait = pin!(me.wait.prepare_to_wait());
            wait.as_mut().add_to_wait_list();

            let thd_to_reap = me.reap_list.lock().pop_front();
            if let Some(thd_to_reap) = thd_to_reap {
                me.reap(thd_to_reap).await;
                continue;
            }

            wait.await;
        }
    }
}
