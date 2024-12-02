use alloc::{
    collections::btree_map::BTreeMap,
    sync::{Arc, Weak},
};
use bindings::EPERM;

use crate::{
    kernel::Terminal,
    prelude::*,
    sync::{AsRefMutPosition as _, AsRefPosition as _, RefMutPosition, RefPosition},
};

use super::{Process, ProcessGroup, ProcessList, Signal, Thread};

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
    job_control: RwSemaphore<SessionJobControl>,

    groups: Locked<BTreeMap<u32, Weak<ProcessGroup>>, ProcessList>,
}

impl Session {
    /// Create a session and add it to the global session list.
    pub(super) fn new(procs: &mut ProcessList, leader: &Arc<Process>) -> Arc<Self> {
        let session = Arc::new(Self {
            sid: leader.pid,
            leader: Arc::downgrade(leader),
            job_control: RwSemaphore::new(SessionJobControl {
                foreground: Weak::new(),
                control_terminal: None,
            }),
            groups: Locked::new(
                BTreeMap::new(),
                // SAFETY: `procs` must be the global process list, which won't be moved.
                procs,
            ),
        });

        procs.add_session(&session);
        session
    }

    pub(super) fn new_group(
        self: &Arc<Self>,
        procs: &mut ProcessList,
        leader: &Arc<Process>,
    ) -> Arc<ProcessGroup> {
        let pgroup = ProcessGroup::new(leader, Arc::downgrade(self), procs);
        let groups = self.groups.access_mut(procs.as_pos_mut());
        assert!(groups
            .insert(pgroup.pgid, Arc::downgrade(&pgroup))
            .is_none());

        pgroup
    }

    pub(super) fn remove_member(&self, pgid: u32, procs: RefMutPosition<'_, ProcessList>) {
        assert!(self.groups.access_mut(procs).remove(&pgid).is_some());
    }

    pub fn foreground(&self) -> Option<Arc<ProcessGroup>> {
        self.job_control.lock_shared().foreground.upgrade()
    }

    /// Set the foreground process group identified by `pgid`.
    /// The process group must belong to the session.
    pub fn set_foreground_pgid(
        &self,
        pgid: u32,
        procs: RefPosition<'_, ProcessList>,
    ) -> KResult<()> {
        if let Some(group) = self.groups.access(procs).get(&pgid) {
            self.job_control.lock().foreground = group.clone();
            Ok(())
        } else {
            // TODO: Check if the process group refers to an existing process group.
            //       That's not a problem though, the operation will fail anyway.
            Err(EPERM)
        }
    }

    /// Only session leaders can set the control terminal.
    /// Make sure we've checked that before calling this function.
    pub fn set_control_terminal(
        self: &Arc<Self>,
        terminal: &Arc<Terminal>,
        forced: bool,
        procs: RefPosition<'_, ProcessList>,
    ) -> KResult<()> {
        let mut job_control = self.job_control.lock();
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
    pub fn drop_control_terminal(&self) -> Option<Arc<Terminal>> {
        let mut inner = self.job_control.lock();
        inner.foreground = Weak::new();
        inner.control_terminal.take()
    }

    pub fn raise_foreground(&self, signal: Signal) {
        if let Some(fg) = self.foreground() {
            let procs = ProcessList::get().lock_shared();
            fg.raise(signal, procs.as_pos());
        }
    }
}
