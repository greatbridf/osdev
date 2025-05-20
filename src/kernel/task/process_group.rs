use super::{Process, ProcessList, Session, Signal};
use alloc::{
    collections::btree_map::BTreeMap,
    sync::{Arc, Weak},
};
use eonix_sync::{Locked, Proof, ProofMut};

pub struct ProcessGroupBuilder {
    pgid: Option<u32>,
    leader: Option<Weak<Process>>,
    session: Option<Arc<Session>>,
}

#[derive(Debug)]
pub struct ProcessGroup {
    pub pgid: u32,
    pub _leader: Weak<Process>,
    pub session: Weak<Session>,

    pub processes: Locked<BTreeMap<u32, Weak<Process>>, ProcessList>,
}

impl ProcessGroupBuilder {
    pub const fn new() -> Self {
        Self {
            pgid: None,
            leader: None,
            session: None,
        }
    }

    pub fn leader(mut self, leader: &Arc<Process>) -> Self {
        self.pgid = Some(leader.pid);
        self.leader = Some(Arc::downgrade(leader));
        self
    }

    pub fn session(mut self, session: Arc<Session>) -> Self {
        self.session = Some(session);
        self
    }

    pub fn build(self, process_list: &mut ProcessList) -> Arc<ProcessGroup> {
        let pgid = self.pgid.expect("PGID is not set");
        let leader = self.leader.expect("Leader is not set");
        let session = self.session.expect("Session is not set");

        let pgroup = Arc::new(ProcessGroup {
            pgid,
            session: Arc::downgrade(&session),
            processes: Locked::new(BTreeMap::from([(pgid, leader.clone())]), process_list),
            _leader: leader,
        });

        process_list.add_pgroup(&pgroup);
        session.add_member(process_list, &pgroup);
        pgroup
    }
}

impl ProcessGroup {
    pub(super) fn add_member(&self, process: &Arc<Process>, procs: ProofMut<'_, ProcessList>) {
        assert!(self
            .processes
            .access_mut(procs)
            .insert(process.pid, Arc::downgrade(process))
            .is_none());
    }

    pub(super) fn remove_member(&self, pid: u32, procs: ProofMut<'_, ProcessList>) {
        let processes = self.processes.access_mut(procs);
        assert!(processes.remove(&pid).is_some());
        if processes.is_empty() {
            self.session
                .upgrade()
                .unwrap()
                .remove_member(self.pgid, procs);
        }
    }

    pub fn raise(&self, signal: Signal, procs: Proof<'_, ProcessList>) {
        let processes = self.processes.access(procs);
        for process in processes.values().map(|p| p.upgrade().unwrap()) {
            process.raise(signal, procs);
        }
    }
}
