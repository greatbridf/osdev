use core::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug)]
pub struct TaskState(AtomicU32);

impl TaskState {
    pub const RUNNING: u32 = 0;
    pub const ISLEEP: u32 = 1;
    pub const USLEEP: u32 = 2;

    pub const fn new(state: u32) -> Self {
        Self(AtomicU32::new(state))
    }

    pub fn swap(&self, state: u32) -> u32 {
        self.0.swap(state, Ordering::AcqRel)
    }

    pub fn cmpxchg(&self, current: u32, new: u32) -> u32 {
        self.0
            .compare_exchange(current, new, Ordering::AcqRel, Ordering::Relaxed)
            .unwrap_or_else(|x| x)
    }

    pub fn is_runnable(&self) -> bool {
        self.0.load(Ordering::Acquire) == Self::RUNNING
    }
}
