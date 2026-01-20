use alloc::collections::vec_deque::VecDeque;
use alloc::sync::Arc;
use core::pin::pin;

use eonix_runtime::scheduler::RUNTIME;
use eonix_sync::{AsProof as _, AsProofMut as _, RwLock, Spin, WaitList};
use intrusive_collections::RBTree;

use super::loader::LoadInfo;
use super::process::AllProcs;
use super::process_group::AllGroups;
use super::session::AllSessions;
use super::thread::AllThreads;
use super::{
    alloc_pid, Process, ProcessBuilder, ProcessGroup, Session, Thread,
    ThreadBuilder, WaitObject,
};
use crate::rcu::call_rcu;

pub struct ProcessList {
    /// The init process.
    init: Option<Arc<Process>>,
    /// All threads.
    threads: RBTree<AllThreads>,
    /// All processes.
    procs: RBTree<AllProcs>,
    /// All process groups.
    pgroups: RBTree<AllGroups>,
    /// All sessions.
    sessions: RBTree<AllSessions>,
}

static GLOBAL_PROC_LIST: RwLock<ProcessList> = RwLock::new(ProcessList {
    init: None,
    threads: RBTree::new(AllThreads::NEW),
    procs: RBTree::new(AllProcs::NEW),
    pgroups: RBTree::new(AllGroups::NEW),
    sessions: RBTree::new(AllSessions::NEW),
});

impl ProcessList {
    pub fn get() -> &'static RwLock<Self> {
        &GLOBAL_PROC_LIST
    }

    pub fn add_session(&mut self, session: &Arc<Session>) {
        self.sessions.insert(session.clone());
    }

    pub fn add_pgroup(&mut self, pgroup: &Arc<ProcessGroup>) {
        self.pgroups.insert(pgroup.clone());
    }

    pub fn add_process(&mut self, process: &Arc<Process>) {
        self.procs.insert(process.clone());
    }

    pub fn add_thread(&mut self, thread: &Arc<Thread>) {
        self.threads.insert(thread.clone());
    }

    pub fn remove_process(&mut self, pid: u32) {
        // Thread group leader has the same tid as the pid.
        let Some(_) = self.threads.find_mut(&pid).remove() else {
            panic!("Thread {} not found", pid);
        };

        let Some(proc) = self.procs.find_mut(&pid).remove() else {
            panic!("Process {} not found", pid);
        };

        // SAFETY: `call_rcu` below.
        let session = unsafe { proc.session.swap(None) }.unwrap();
        let pgroup = unsafe { proc.pgroup.swap(None) }.unwrap();
        let parent = unsafe { proc.parent.swap(None) }.unwrap();

        pgroup.remove_member(&proc, self);

        call_rcu(move || {
            drop(session);
            drop(pgroup);
            drop(parent);
        });
    }

    pub fn remove_thread(&mut self, thread: &Arc<Thread>) {
        assert!(
            self.threads.find_mut(&thread.tid).remove().is_some(),
            "Double remove"
        );
    }

    pub fn remove_session(&mut self, session: &Arc<Session>) {
        assert!(
            self.sessions.find_mut(&session.sid).remove().is_some(),
            "Double remove"
        );
    }

    pub fn remove_pgroup(&mut self, pgroup: &Arc<ProcessGroup>) {
        assert!(
            self.pgroups.find_mut(&pgroup.pgid).remove().is_some(),
            "Double remove"
        );
    }

    fn set_init_process(&mut self, init: Arc<Process>) {
        let old_init = self.init.replace(init);
        assert!(old_init.is_none(), "Init process already set");
    }

    pub fn init_process(&self) -> &Arc<Process> {
        self.init.as_ref().unwrap()
    }

    pub fn try_find_thread(&self, tid: u32) -> Option<Arc<Thread>> {
        self.threads.find(&tid).clone_pointer()
    }

    pub fn try_find_process(&self, pid: u32) -> Option<Arc<Process>> {
        self.procs.find(&pid).clone_pointer()
    }

    pub fn try_find_pgroup(&self, pgid: u32) -> Option<Arc<ProcessGroup>> {
        self.pgroups.find(&pgid).clone_pointer()
    }

    pub fn try_find_session(&self, sid: u32) -> Option<Arc<Session>> {
        self.sessions.find(&sid).clone_pointer()
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
            panic!("init exited: {}", alloc_pid());
        }

        let mut procs = ProcessList::get().write().await;

        if thread.tid != process.pid {
            let threads = process.threads.access_mut(procs.prove_mut());
            assert!(
                threads.find_mut(&thread.tid).remove().is_some(),
                "Thread gone?"
            );

            procs.remove_thread(&thread);
        }

        // main thread exit
        if thread.tid == process.pid {
            assert_eq!(thread.tid, process.pid);

            thread.files.close_all().await;

            let session = process.session(procs.prove()).clone();
            // If we are the session leader, we should drop the control terminal.
            if session.sid == process.pid {
                if let Some(terminal) = session.control_terminal() {
                    terminal.drop_session(procs.prove());
                }
            }

            // Release the MMList as well as the page table.
            process.mm_list.release();

            // Make children orphans (adopted by init)
            let init = procs.init_process();
            let children = process.children.access_mut(procs.prove_mut());
            for child in children.take() {
                // XXX: May buggy. Check here again.
                // SAFETY: `child.parent` must be ourself.
                //         So we don't need to free it.
                unsafe { child.parent.swap(Some(init.clone())) };
                init.add_child(&child, procs.prove_mut());
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
