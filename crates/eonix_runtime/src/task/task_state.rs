use core::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug)]
pub struct TaskState(AtomicU32);

impl TaskState {
    pub const BLOCKED: u32 = 0;
    pub const READY: u32 = 1;
    pub const RUNNING: u32 = 2;
    pub const READY_RUNNING: u32 = TaskState::READY | TaskState::RUNNING;
    pub const DEAD: u32 = 1 << 31;

    pub(crate) const fn new(state: u32) -> Self {
        Self(AtomicU32::new(state))
    }

    pub(crate) fn swap(&self, state: u32) -> u32 {
        self.0.swap(state, Ordering::SeqCst)
    }

    pub(crate) fn update(&self, func: impl FnMut(u32) -> Option<u32>) -> Result<u32, u32> {
        self.0
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, func)
    }
}
