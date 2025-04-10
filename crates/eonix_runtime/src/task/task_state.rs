use core::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug)]
pub struct TaskState(AtomicU32);

impl TaskState {
    pub const RUNNING: u32 = 0;
    pub const SLEEPING: u32 = 1;

    pub(crate) const fn new(state: u32) -> Self {
        Self(AtomicU32::new(state))
    }

    pub(crate) fn swap(&self, state: u32) -> u32 {
        self.0.swap(state, Ordering::AcqRel)
    }

    pub(crate) fn is_running(&self) -> bool {
        self.0.load(Ordering::Acquire) == Self::RUNNING
    }
}
