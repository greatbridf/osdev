use super::interrupt::end_of_interrupt;
use core::sync::atomic::{AtomicUsize, Ordering};
use eonix_runtime::scheduler::Scheduler;

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
    end_of_interrupt();
    TICKS.fetch_add(1, Ordering::Relaxed);

    if eonix_preempt::count() == 0 {
        // To make scheduler satisfied.
        eonix_preempt::disable();
        Scheduler::schedule();
    }
}

pub fn ticks() -> Ticks {
    Ticks(TICKS.load(Ordering::Relaxed))
}
