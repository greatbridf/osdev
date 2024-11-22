use core::sync::atomic::{AtomicUsize, Ordering};

use crate::{prelude::*, sync::preempt};

use super::{interrupt::register_irq_handler, task::Scheduler};

static TICKS: AtomicUsize = AtomicUsize::new(0);

pub struct Ticks(usize);

impl Ticks {
    pub fn in_secs(&self) -> usize {
        self.0 / 100
    }

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

fn timer_interrupt() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    if preempt::count() == 0 {
        println_debug!("Timer interrupt reschedule");
        // To make scheduler satisfied.
        preempt::disable();
        Scheduler::schedule();
    }
}

pub fn ticks() -> Ticks {
    Ticks(TICKS.load(Ordering::Relaxed))
}

pub fn init() -> KResult<()> {
    register_irq_handler(0, timer_interrupt)
}
