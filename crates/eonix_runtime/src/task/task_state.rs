use core::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug)]
pub struct TaskState(AtomicU32);

impl TaskState {
    pub const READY: u32 = 0;
    pub const RUNNING: u32 = 1;
    pub const PARKED: u32 = 2;
    pub const DEAD: u32 = 1 << 31;

    pub(crate) const fn new(state: u32) -> Self {
        Self(AtomicU32::new(state))
    }

    pub(crate) fn swap(&self, state: u32) -> u32 {
        self.0.swap(state, Ordering::SeqCst)
    }

    pub(crate) fn set(&self, state: u32) {
        self.0.store(state, Ordering::SeqCst);
    }

    pub(crate) fn get(&self) -> u32 {
        self.0.load(Ordering::SeqCst)
    }

    pub(crate) fn cmpxchg(&self, current: u32, new: u32) -> Result<u32, u32> {
        self.0
            .compare_exchange(current, new, Ordering::SeqCst, Ordering::SeqCst)
    }
}
