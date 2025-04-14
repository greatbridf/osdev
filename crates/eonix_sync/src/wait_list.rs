mod prepare;
mod wait_object;

use crate::{LazyLock, Spin};
use core::{fmt, sync::atomic::Ordering};
use intrusive_collections::LinkedList;
use wait_object::WaitObjectAdapter;

pub use prepare::Prepare;

pub struct WaitList {
    waiters: LazyLock<Spin<LinkedList<WaitObjectAdapter>>>,
}

impl WaitList {
    pub const fn new() -> Self {
        Self {
            waiters: LazyLock::new(|| Spin::new(LinkedList::new(WaitObjectAdapter::new()))),
        }
    }

    pub fn has_waiters(&self) -> bool {
        !self.waiters.lock().is_empty()
    }

    pub fn notify_one(&self) -> bool {
        let mut waiters = self.waiters.lock();
        let mut waiter = waiters.front_mut();
        if let Some(waiter) = waiter.get() {
            // SAFETY: `wait_object` is a valid reference to a `WaitObject` because we
            //         won't drop the wait object until the waiting thread will be woken
            //         up and make sure that it is not on the list.
            waiter.woken_up.store(true, Ordering::Release);

            if let Some(waker) = waiter.waker.lock().take() {
                waker.wake();
            }
        }

        // We need to remove the node from the list AFTER we've finished accessing it so
        // the waiter knows when it is safe to release the wait object node.
        waiter.remove().is_some()
    }

    pub fn notify_all(&self) -> usize {
        let mut waiters = self.waiters.lock();
        let mut waiter = waiters.front_mut();
        let mut count = 0;

        while !waiter.is_null() {
            if let Some(waiter) = waiter.get() {
                // SAFETY: `wait_object` is a valid reference to a `WaitObject` because we
                //         won't drop the wait object until the waiting thread will be woken
                //         up and make sure that it is not on the list.
                waiter.woken_up.store(true, Ordering::Release);

                if let Some(waker) = waiter.waker.lock().take() {
                    waker.wake();
                }
            } else {
                unreachable!("Invalid state.");
            }

            count += 1;

            // We need to remove the node from the list AFTER we've finished accessing it so
            // the waiter knows when it is safe to release the wait object node.
            waiter.remove();
        }

        count
    }

    pub fn prepare_to_wait(&self) -> Prepare<'_> {
        Prepare::new(self)
    }
}

impl Default for WaitList {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for WaitList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WaitList").finish()
    }
}
