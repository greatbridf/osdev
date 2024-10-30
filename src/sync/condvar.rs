use alloc::collections::vec_deque::VecDeque;
use bindings::{
    current_thread,
    kernel::task::{thread, thread_ISLEEP, thread_READY, thread_USLEEP},
    schedule_now_preempt_disabled,
};

use crate::{prelude::*, sync::preempt_disable};

use super::{lock::Guard, strategy::LockStrategy};

/// `current` should be per CPU, so no sync is needed
fn current() -> &'static mut *mut thread {
    #[allow(static_mut_refs)]
    unsafe {
        &mut current_thread
    }
}

pub struct CondVar {
    waiters: Spin<VecDeque<*mut thread>>,
}

// TODO!!!: acquire dispatcher lock because modifying thread attribute
//          is racy. But we put this in the future work since that would
//          require a lot of changes in the kernel task management system.
unsafe impl Send for CondVar {}
unsafe impl Sync for CondVar {}

impl CondVar {
    pub fn new() -> Self {
        Self {
            waiters: Spin::new(VecDeque::new()),
        }
    }

    pub fn notify_one(&self) {
        // TODO!!!: acquire dispatcher lock
        let mut waiters = self.waiters.lock();

        if waiters.is_empty() {
            return;
        }

        let thread = waiters
            .pop_front()
            .map(|ptr| unsafe { ptr.as_mut() }.unwrap());

        if let Some(thread) = thread {
            unsafe { thread.set_attr(thread_READY, true) };
        }
    }

    pub fn notify_all(&self) {
        // TODO!!!: acquire dispatcher lock
        let mut waiters = self.waiters.lock();

        if waiters.is_empty() {
            return;
        }

        for item in waiters.iter() {
            let thread = unsafe { item.as_mut() }.unwrap();
            unsafe { thread.set_attr(thread_READY, true) };
        }

        waiters.clear();
    }

    /// # Might Sleep
    /// This function **might sleep**, so call it in a preemptible context
    ///
    /// # Return
    /// - `true`: a pending signal was received
    pub fn wait<'a, T, S: LockStrategy>(
        &self,
        guard: &mut Guard<'a, T, S>,
        interruptible: bool,
    ) -> bool {
        preempt_disable();

        // TODO!!!: acquire dispatcher lock
        let current = *current();

        let current_mut = unsafe { current.as_mut() }.unwrap();
        unsafe {
            if interruptible {
                current_mut.set_attr(thread_ISLEEP, false);
            } else {
                current_mut.set_attr(thread_USLEEP, false);
            }
        }

        {
            let mut waiters = self.waiters.lock();
            waiters.push_back(current);
        }

        unsafe {
            guard.force_unlock();
        }

        might_sleep!(1);

        let has_signals = unsafe { schedule_now_preempt_disabled() };

        unsafe {
            guard.force_relock();
        }

        has_signals
    }
}
