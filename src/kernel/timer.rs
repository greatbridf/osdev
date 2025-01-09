use core::sync::atomic::{AtomicUsize, Ordering};

use crate::sync::preempt;

use super::{interrupt::end_of_interrupt, task::Scheduler};

static TICKS: AtomicUsize = AtomicUsize::new(0);

pub struct Ticks(usize);

impl Ticks {
    pub fn in_secs(&self) -> usize {
        self.0 / 100
    }

    #[allow(dead_code)]
    pub fn in_msecs(&self) -> usize {
        self.0 * 10
    }

    pub fn in_usecs(&self) -> usize {
        self.0 * 10_000
    }

    pub fn in_nsecs(&self) -> usize {
        self.0 * 10_000_000
    }
}

pub fn timer_interrupt() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    if preempt::count() == 0 {
        // To make scheduler satisfied.
        preempt::disable();
        end_of_interrupt();
        Scheduler::schedule();
    } else {
        end_of_interrupt();
    }
}

pub fn ticks() -> Ticks {
    Ticks(TICKS.load(Ordering::Relaxed))
}
