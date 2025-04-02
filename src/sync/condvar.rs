use core::task::Waker;

use crate::{
    kernel::{
        console::println_trace,
        task::{Scheduler, Task},
    },
    prelude::*,
    sync::preempt,
};

use super::{lock::Guard, strategy::LockStrategy};
use alloc::collections::vec_deque::VecDeque;

pub struct CondVar<const INTERRUPTIBLE: bool> {
    waiters: Spin<VecDeque<Waker>>,
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

    fn wake(waker: Waker) {
        println_trace!("trace_condvar", "tid({}) is trying to wake", thread.tid);
        waker.wake();
        println_trace!("trace_condvar", "tid({}) is awake", thread.tid);
    }

    fn sleep() -> Waker {
        let task = Task::current();

        println_trace!("trace_condvar", "tid({}) is trying to sleep", task.id);

        let waker = if I {
            Waker::from(task.isleep())
        } else {
            Waker::from(task.usleep())
        };

        println_trace!("trace_condvar", "tid({}) is sleeping", task.id);

        waker
    }

    pub fn notify_one(&self) {
        if let Some(waker) = self.waiters.lock().pop_front() {
            Self::wake(waker);
        }
    }

    pub fn notify_all(&self) {
        for waker in self.waiters.lock().drain(..) {
            Self::wake(waker);
        }
    }

    /// Unlock the `guard`. Then wait until being waken up. Relock the `guard` before returning.
    ///
    /// # Might Sleep
    /// This function **might sleep**, so call it in a preemptible context.
    pub fn wait<'a, T, S: LockStrategy, const W: bool>(&self, guard: &mut Guard<'a, T, S, W>) {
        preempt::disable();
        let waker = Self::sleep();
        self.waiters.lock().push_back(waker);

        // TODO!!!: Another way to do this:
        //
        // Store a flag in our entry in the waiting list.
        // Check the flag before doing `schedule()` but after we've unlocked the `guard`.
        // If the flag is already set, we don't need to sleep.

        unsafe { guard.force_unlock() };

        might_sleep!(1);
        Scheduler::schedule();

        unsafe { guard.force_relock() };

        assert!(Task::current().is_runnable());
    }
}
