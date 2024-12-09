use alloc::{
    collections::btree_map::BTreeMap,
    sync::{Arc, Weak},
};

use crate::{
    prelude::*,
    sync::{RefMutPosition, RefPosition},
};

use super::{Process, ProcessList, Session, Signal};

#[allow(dead_code)]
#[derive(Debug)]
pub struct ProcessGroup {
    pub pgid: u32,
    pub leader: Weak<Process>,
    pub session: Weak<Session>,

    pub processes: Locked<BTreeMap<u32, Weak<Process>>, ProcessList>,
}

impl ProcessGroup {
    /// Don't use this function directly. Use `Session::new_group` instead.
    pub(super) fn new(
        leader: &Arc<Process>,
        session: Weak<Session>,
        procs: &mut ProcessList,
    ) -> Arc<Self> {
        let pgroup = Arc::new(Self {
            pgid: leader.pid,
            leader: Arc::downgrade(leader),
            session,
            processes: Locked::new(
                BTreeMap::from([(leader.pid, Arc::downgrade(leader))]),
                // SAFETY: `procs` must be the global process list, which won't be moved.
                procs,
            ),
        });

        procs.add_pgroup(&pgroup);
        pgroup
    }

    pub(super) fn add_member(
        &self,
        process: &Arc<Process>,
        procs: RefMutPosition<'_, ProcessList>,
    ) {
        assert!(self
            .processes
            .access_mut(procs)
            .insert(process.pid, Arc::downgrade(process))
            .is_none());
    }

    pub(super) fn remove_member(&self, pid: u32, procs: RefMutPosition<'_, ProcessList>) {
        let processes = self.processes.access_mut(procs);
        assert!(processes.remove(&pid).is_some());
        if processes.is_empty() {
            self.session
                .upgrade()
                .unwrap()
                .remove_member(self.pgid, procs);
        }
    }

    pub fn raise(&self, signal: Signal, procs: RefPosition<'_, ProcessList>) {
        let processes = self.processes.access(procs);
        for process in processes.values().map(|p| p.upgrade().unwrap()) {
            process.raise(signal, procs);
        }
    }
}
