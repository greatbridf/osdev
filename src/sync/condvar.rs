use crate::{kernel::task::Thread, prelude::*};
use alloc::collections::vec_deque::VecDeque;
use core::task::Waker;
use eonix_preempt::assert_preempt_count_eq;
use eonix_runtime::task::Task;
use eonix_sync::ForceUnlockableGuard;

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
    pub const fn new() -> Self {
        Self {
            waiters: Spin::new(VecDeque::new()),
        }
    }

    pub fn has_waiters(&self) -> bool {
        !self.waiters.lock().is_empty()
    }

    fn wake(waker: Waker) {
        waker.wake();
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
    pub fn wait(&self, guard: &mut impl ForceUnlockableGuard) {
        eonix_preempt::disable();
        let waker = Waker::from(Task::current().clone());
        if I {
            // Prohibit the thread from being woken up by a signal.
            Thread::current()
                .signal_list
                .set_signal_waker(Some(waker.clone()));
        }

        self.waiters.lock().push_back(waker.clone());

        // TODO!!!: Another way to do this:
        //
        // Store a flag in our entry in the waiting list.
        // Check the flag before doing `schedule()` but after we've unlocked the `guard`.
        // If the flag is already set, we don't need to sleep.

        unsafe { guard.force_unlock() };

        assert_preempt_count_eq!(1, "CondVar::wait");
        Task::park_preempt_disabled();

        if I {
            // Allow the thread to be woken up by a signal again.
            Thread::current().signal_list.set_signal_waker(None);
        }

        unsafe { guard.force_relock() };
    }
}
