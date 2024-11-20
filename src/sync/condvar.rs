use crate::{
    kernel::task::{Scheduler, Thread},
    prelude::*,
    sync::preempt,
};

use super::{lock::Guard, strategy::LockStrategy};
use alloc::{collections::vec_deque::VecDeque, sync::Arc};

pub struct CondVar<const INTERRUPTIBLE: bool> {
    waiters: Spin<VecDeque<Arc<Thread>>>,
}

impl<const I: bool> core::fmt::Debug for CondVar<I> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if I {
            f.debug_struct("CondVar").finish()
        } else {
            f.debug_struct("CondVarUnintrruptible").finish()
        }
    }
}

impl<const I: bool> CondVar<I> {
    pub fn new() -> Self {
        Self {
            waiters: Spin::new(VecDeque::new()),
        }
    }

    fn wake(schedule: &mut Scheduler, thread: &Arc<Thread>) {
        if I {
            schedule.iwake(thread);
        } else {
            schedule.uwake(thread);
        }
    }

    fn sleep(scheduler: &mut Scheduler) {
        if I {
            scheduler.isleep(Thread::current());
        } else {
            scheduler.usleep(Thread::current());
        }
    }

    pub fn notify_one(&self) {
        let mut scheduler = Scheduler::get().lock_irq();
        if let Some(waiter) = self.waiters.lock().pop_front() {
            Self::wake(scheduler.as_mut(), &waiter);
        }
    }

    pub fn notify_all(&self) {
        let mut scheduler = Scheduler::get().lock_irq();
        self.waiters.lock().retain(|waiter| {
            Self::wake(scheduler.as_mut(), &waiter);
            false
        });
    }

    /// Unlock the `guard`. Then wait until being waken up. Relock the `guard` before returning.
    ///
    /// # Might Sleep
    /// This function **might sleep**, so call it in a preemptible context.
    ///
    /// # Return
    /// - `true`: a pending signal was received
    pub fn wait<'a, T, S: LockStrategy>(&self, guard: &mut Guard<'a, T, S>) {
        preempt::disable();
        {
            let mut scheduler = Scheduler::get().lock_irq();
            // We have scheduler locked and IRQ disabled. So no one could be waking us up for now.

            self.waiters.lock().push_back(Thread::current().clone());
            Self::sleep(scheduler.as_mut());
        }

        // TODO!!!: Another way to do this:
        //
        // Store a flag in our entry in the waiting list.
        // Check the flag before doing `schedule()` but after we've unlocked the `guard`.
        // If the flag is already set, we don't need to sleep.

        unsafe { guard.force_unlock() };
        Scheduler::schedule();
        unsafe { guard.force_relock() };
    }
}
