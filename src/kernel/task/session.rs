use super::{Process, ProcessGroup, ProcessList, Signal, Thread};
use crate::kernel::constants::EPERM;
use crate::{kernel::Terminal, prelude::*};
use alloc::{
    collections::btree_map::BTreeMap,
    sync::{Arc, Weak},
};
use eonix_sync::{AsProof as _, AsProofMut as _, Locked, Proof, ProofMut, RwLock};

#[derive(Debug)]
struct SessionJobControl {
    /// Foreground process group
    foreground: Weak<ProcessGroup>,
    control_terminal: Option<Arc<Terminal>>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Session {
    pub sid: u32,
    pub leader: Weak<Process>,
    job_control: RwLock<SessionJobControl>,

    groups: Locked<BTreeMap<u32, Weak<ProcessGroup>>, ProcessList>,
}

impl Session {
    /// Create a session and add it to the global session list.
    pub fn new(leader: &Arc<Process>, process_list: &mut ProcessList) -> Arc<Self> {
        let session = Arc::new(Self {
            sid: leader.pid,
            leader: Arc::downgrade(leader),
            job_control: RwLock::new(SessionJobControl {
                foreground: Weak::new(),
                control_terminal: None,
            }),
            groups: Locked::new(
                BTreeMap::new(),
                // SAFETY: `procs` must be the global process list, which won't be moved.
                process_list,
            ),
        });

        process_list.add_session(&session);
        session
    }

    pub(super) fn add_member(&self, procs: &mut ProcessList, pgroup: &Arc<ProcessGroup>) {
        let groups = self.groups.access_mut(procs.prove_mut());
        let old = groups.insert(pgroup.pgid, Arc::downgrade(pgroup));
        assert!(old.is_none(), "Process group already exists");
    }

    pub(super) fn remove_member(&self, pgid: u32, procs: ProofMut<'_, ProcessList>) {
        assert!(self.groups.access_mut(procs).remove(&pgid).is_some());
    }

    pub async fn foreground(&self) -> Option<Arc<ProcessGroup>> {
        self.job_control.read().await.foreground.upgrade()
    }

    /// Set the foreground process group identified by `pgid`.
    /// The process group must belong to the session.
    pub async fn set_foreground_pgid(
        &self,
        pgid: u32,
        procs: Proof<'_, ProcessList>,
    ) -> KResult<()> {
        if let Some(group) = self.groups.access(procs).get(&pgid) {
            self.job_control.write().await.foreground = group.clone();
            Ok(())
        } else {
            // TODO: Check if the process group refers to an existing process group.
            //       That's not a problem though, the operation will fail anyway.
            Err(EPERM)
        }
    }

    /// Only session leaders can set the control terminal.
    /// Make sure we've checked that before calling this function.
    pub async fn set_control_terminal(
        self: &Arc<Self>,
        terminal: &Arc<Terminal>,
        forced: bool,
        procs: Proof<'_, ProcessList>,
    ) -> KResult<()> {
        let mut job_control = self.job_control.write().await;
        if let Some(_) = job_control.control_terminal.as_ref() {
            if let Some(session) = terminal.session().as_ref() {
                if session.sid == self.sid {
                    return Ok(());
                }
            }
            return Err(EPERM);
        }
        terminal.set_session(self, forced)?;
        job_control.control_terminal = Some(terminal.clone());
        job_control.foreground = Arc::downgrade(&Thread::current().process.pgroup(procs));
        Ok(())
    }

    /// Drop the control terminal reference inside the session.
    /// DO NOT TOUCH THE TERMINAL'S SESSION FIELD.
    pub async fn drop_control_terminal(&self) -> Option<Arc<Terminal>> {
        let mut inner = self.job_control.write().await;
        inner.foreground = Weak::new();
        inner.control_terminal.take()
    }

    pub async fn raise_foreground(&self, signal: Signal) {
        if let Some(fg) = self.foreground().await {
            let procs = ProcessList::get().read().await;
            fg.raise(signal, procs.prove());
        }
    }
}
