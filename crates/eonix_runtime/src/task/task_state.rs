use core::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug)]
pub struct TaskState(AtomicU32);

impl TaskState {
    pub const RUNNING: u32 = 0;
    pub const PARKING: u32 = 1;
    pub const PARKED: u32 = 2;

    pub(crate) const fn new(state: u32) -> Self {
        Self(AtomicU32::new(state))
    }

    pub(crate) fn swap(&self, state: u32) -> u32 {
        self.0.swap(state, Ordering::AcqRel)
    }

    pub(crate) fn try_park(&self) -> bool {
        match self.0.compare_exchange(
            TaskState::PARKING,
            TaskState::PARKED,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => true,
            Err(TaskState::RUNNING) => false,
            Err(_) => unreachable!("Invalid task state while trying to park."),
        }
    }

    pub(crate) fn is_running(&self) -> bool {
        self.0.load(Ordering::Acquire) == Self::RUNNING
    }
}
