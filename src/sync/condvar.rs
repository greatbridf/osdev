use crate::{
    kernel::{
        console::println_trace,
        task::{Scheduler, Thread, ThreadState},
    },
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

    fn wake(thread: &Arc<Thread>) {
        println_trace!("trace_condvar", "tid({}) is trying to wake", thread.tid);
        if I {
            thread.iwake();
        } else {
            thread.uwake();
        }
        println_trace!("trace_condvar", "tid({}) is awake", thread.tid);
    }

    fn sleep() {
        let thread = Thread::current();
        println_trace!("trace_condvar", "tid({}) is trying to sleep", thread.tid);
        if I {
            thread.isleep();
        } else {
            thread.usleep();
        }
        println_trace!("trace_condvar", "tid({}) is sleeping", thread.tid);
    }

    pub fn notify_one(&self) {
        if let Some(waiter) = self.waiters.lock().pop_front() {
            Self::wake(&waiter);
        }
    }

    pub fn notify_all(&self) {
        self.waiters.lock().retain(|waiter| {
            Self::wake(&waiter);
            false
        });
    }

    /// Unlock the `guard`. Then wait until being waken up. Relock the `guard` before returning.
    ///
    /// # Might Sleep
    /// This function **might sleep**, so call it in a preemptible context.
    pub fn wait<'a, T, S: LockStrategy, const W: bool>(&self, guard: &mut Guard<'a, T, S, W>) {
        preempt::disable();
        self.waiters.lock().push_back(Thread::current().clone());
        Self::sleep();

        // TODO!!!: Another way to do this:
        //
        // Store a flag in our entry in the waiting list.
        // Check the flag before doing `schedule()` but after we've unlocked the `guard`.
        // If the flag is already set, we don't need to sleep.

        unsafe { guard.force_unlock() };
        Scheduler::schedule();
        unsafe { guard.force_relock() };

        Thread::current().state.assert(ThreadState::RUNNING);

        self.waiters
            .lock()
            .retain(|waiter| waiter.tid != Thread::current().tid);
    }
}
