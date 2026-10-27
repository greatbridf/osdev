use alloc::sync::{Arc, Weak};

use eonix_sync::{AsProof as _, AsProofMut, Locked, Proof, ProofMut};
use intrusive_collections::{
    intrusive_adapter, KeyAdapter, RBTree, RBTreeAtomicLink,
};
use posix_types::signal::Signal;

use super::process_group::SessionGroups;
use super::{Process, ProcessGroup, ProcessList};
use crate::kernel::constants::EPERM;
use crate::kernel::Terminal;
use crate::prelude::*;

struct SessionJobControl {
    foreground: Option<Arc<ProcessGroup>>,
    control_terminal: Option<Arc<Terminal>>,
}

pub struct Session {
    pub sid: u32,
    pub leader: Weak<Process>,
    job_control: Spin<SessionJobControl>,
    groups: Locked<RBTree<SessionGroups>, ProcessList>,
    all_sessions_link: RBTreeAtomicLink,
}

intrusive_adapter!(pub AllSessions = Arc<Session>: Session {
    all_sessions_link: RBTreeAtomicLink
});

impl KeyAdapter<'_> for AllSessions {
    type Key = u32;

    fn get_key(
        &self,
        value: &'_ <Self::PointerOps as intrusive_collections::PointerOps>::Value,
    ) -> Self::Key {
        value.sid
    }
}

impl Session {
    /// Create a session and add it to the global session list.
    pub fn new(leader: &Arc<Process>, proclist: &mut ProcessList) -> Arc<Self> {
        let session = Arc::new(Self {
            sid: leader.pid,
            leader: Arc::downgrade(leader),
            job_control: Spin::new(SessionJobControl {
                foreground: None,
                control_terminal: None,
            }),
            groups: Locked::new(RBTree::new(SessionGroups::NEW), proclist),
            all_sessions_link: RBTreeAtomicLink::new(),
        });

        proclist.add_session(&session);
        session
    }

    pub fn add_member(
        &self,
        pgroup: &Arc<ProcessGroup>,
        procs: ProofMut<'_, ProcessList>,
    ) {
        assert!(self.all_sessions_link.is_linked(), "Dead session");
        self.groups.access_mut(procs).insert(pgroup.clone());
    }

    pub fn remove_member(
        self: &Arc<Self>,
        pgroup: &Arc<ProcessGroup>,
        procs: &mut ProcessList,
    ) {
        let members = self.groups.access_mut(procs.prove_mut());
        assert!(
            members.find_mut(&pgroup.pgid).remove().is_some(),
            "Not a member"
        );

        if let Some(fg_pgroup) = self.foreground_pgroup() {
            if fg_pgroup.pgid == pgroup.pgid {
                let _ = self.set_foreground_pgroup(None);
            }
        }

        if !members.is_empty() {
            return;
        }

        // Recycle dead session.
        procs.remove_session(self);
    }

    pub fn leader(&self) -> Option<Arc<Process>> {
        self.leader.upgrade()
    }

    pub fn foreground_pgroup(&self) -> Option<Arc<ProcessGroup>> {
        self.job_control.lock().foreground.clone()
    }

    pub fn control_terminal(&self) -> Option<Arc<Terminal>> {
        self.job_control.lock().control_terminal.clone()
    }

    /// Set the foreground process group identified by `pgid`.
    /// The process group must belong to the session.
    pub fn set_foreground_pgroup(
        &self,
        pgroup: Option<&Arc<ProcessGroup>>,
    ) -> KResult<()> {
        if let Some(pgroup) = pgroup {
            if pgroup.session.sid != self.sid {
                return Err(EPERM);
            }
        }

        self.job_control.lock().foreground = pgroup.cloned();
        Ok(())
    }

    /// Set our controlling terminal to `terminal`. Only meant to be called by
    /// the session leader. The pgroup that the session leader is in becomes the
    /// new foreground pgroup.
    ///
    /// # Panics
    /// Panics if we have a controlling terminal already
    /// or the session leader is gone.
    pub fn _set_control_terminal(
        self: &Arc<Self>,
        terminal: &Arc<Terminal>,
        procs: Proof<'_, ProcessList>,
    ) {
        let mut job_control = self.job_control.lock();
        let leader = self.leader().expect("Leader is gone?");

        assert!(
            job_control.control_terminal.is_none(),
            "We have a controlling terminal already"
        );

        job_control.control_terminal = Some(terminal.clone());
        job_control.foreground = Some(leader.pgroup(procs).clone());
    }

    /// Drop the control terminal reference inside the session.
    /// Send SIGHUP and then SIGCONT to our foreground pgroup.
    pub fn _drop_control_terminal(&self, procs: Proof<'_, ProcessList>) {
        let foreground = {
            let mut inner = self.job_control.lock();
            inner.control_terminal = None;
            inner.foreground.take()
        };

        if let Some(foreground) = foreground {
            foreground.raise(Signal::SIGHUP, procs);
            foreground.raise(Signal::SIGCHLD, procs);
        }
    }

    pub async fn raise_foreground(&self, signal: Signal) {
        let Some(fg) = self.foreground_pgroup() else {
            return;
        };

        let procs = ProcessList::get().read().await;
        fg.raise(signal, procs.prove());
    }
}
