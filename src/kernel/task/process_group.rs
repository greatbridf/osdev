use alloc::sync::{Arc, Weak};

use eonix_sync::{AsProofMut, Locked, Proof, ProofMut};
use intrusive_collections::{
    intrusive_adapter, KeyAdapter, RBTree, RBTreeAtomicLink,
};
use posix_types::signal::Signal;

use super::process::GroupProcs;
use super::{Process, ProcessList, Session};

pub struct ProcessGroup {
    pub pgid: u32,
    pub leader: Weak<Process>,
    pub session: Arc<Session>,

    pub procs: Locked<RBTree<GroupProcs>, ProcessList>,

    all_groups_link: RBTreeAtomicLink,
    session_groups_link: RBTreeAtomicLink,
}

intrusive_adapter!(pub AllGroups = Arc<ProcessGroup>: ProcessGroup {
    all_groups_link: RBTreeAtomicLink
});
intrusive_adapter!(pub SessionGroups = Arc<ProcessGroup>: ProcessGroup {
    session_groups_link: RBTreeAtomicLink
});

impl KeyAdapter<'_> for AllGroups {
    type Key = u32;

    fn get_key(
        &self,
        value: &'_ <Self::PointerOps as intrusive_collections::PointerOps>::Value,
    ) -> Self::Key {
        value.pgid
    }
}

impl KeyAdapter<'_> for SessionGroups {
    type Key = u32;

    fn get_key(
        &self,
        value: &'_ <Self::PointerOps as intrusive_collections::PointerOps>::Value,
    ) -> Self::Key {
        value.pgid
    }
}

impl ProcessGroup {
    /// Create a pgroup and add it to the global pgroup list.
    /// Add the pgroup to the session.
    ///
    /// # Panics
    /// Panics if `leader` is already in some pgroup.
    pub fn new(
        leader: &Arc<Process>,
        session: &Arc<Session>,
        procs: &mut ProcessList,
    ) -> Arc<Self> {
        let pgid = leader.pid;
        let pgroup_procs = {
            let mut list = RBTree::new(GroupProcs::new());
            list.insert(leader.clone());
            list
        };

        let pgroup = Arc::new(ProcessGroup {
            pgid,
            session: session.clone(),
            procs: Locked::new(pgroup_procs, procs),
            leader: Arc::downgrade(leader),
            all_groups_link: RBTreeAtomicLink::new(),
            session_groups_link: RBTreeAtomicLink::new(),
        });

        procs.add_pgroup(&pgroup);
        session.add_member(&pgroup, procs.prove_mut());
        pgroup
    }

    /// Add `process` to the pgroup.
    ///
    /// # Panics
    /// Panics if `process` is already in some pgroup or the pgroup is dead.
    pub fn add_member(
        &self,
        process: &Arc<Process>,
        procs: ProofMut<'_, ProcessList>,
    ) {
        assert!(self.all_groups_link.is_linked(), "Dead pgroup");
        self.procs.access_mut(procs).insert(process.clone());
    }

    pub fn remove_member(
        self: &Arc<Self>,
        process: &Arc<Process>,
        procs: &mut ProcessList,
    ) {
        let members = self.procs.access_mut(procs.prove_mut());
        assert!(
            members.find_mut(&process.pid).remove().is_some(),
            "Not a member"
        );

        if !members.is_empty() {
            return;
        }

        self.session.remove_member(self, procs);
        procs.remove_pgroup(self);
    }

    pub fn raise(&self, signal: Signal, procs: Proof<'_, ProcessList>) {
        let members = self.procs.access(procs);
        for process in members.iter() {
            process.raise(signal, procs);
        }
    }
}
