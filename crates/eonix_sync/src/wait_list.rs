mod wait_handle;
mod wait_object;

use crate::{LazyLock, Spin};
use core::fmt;
use intrusive_collections::{linked_list::CursorMut, LinkedList};
use wait_object::{WaitObject, WaitObjectAdapter};

pub use wait_handle::WaitHandle;

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

        if !waiter.is_null() {
            unsafe {
                // SAFETY: `waiter` is not null.
                self.notify_waiter_unchecked(&mut waiter);
            }

            true
        } else {
            false
        }
    }

    pub fn notify_all(&self) -> usize {
        let mut waiters = self.waiters.lock();
        let mut waiter = waiters.front_mut();
        let mut count = 0;

        while !waiter.is_null() {
            unsafe {
                // SAFETY: `waiter` is not null.
                self.notify_waiter_unchecked(&mut waiter);
            }
            count += 1;
        }

        count
    }

    pub fn prepare_to_wait(&self) -> WaitHandle<'_> {
        WaitHandle::new(self)
    }
}

impl WaitList {
    unsafe fn notify_waiter_unchecked(&self, waiter: &mut CursorMut<'_, WaitObjectAdapter>) {
        let wait_object = unsafe {
            // SAFETY: The caller guarantees that `waiter` should be `Some`.
            //         `wait_object` is a valid reference to a `WaitObject` because we
            //         won't drop the wait object until the waiting thread will be woken
            //         up and make sure that it is not on the list.
            waiter.get().unwrap_unchecked()
        };

        wait_object.set_woken_up();

        if let Some(waker) = wait_object.take_waker() {
            waker.wake();
        }

        // Acknowledge the wait object that we're done.
        unsafe {
            waiter.remove().unwrap_unchecked().clear_wait_list();
        }
    }

    pub(self) fn notify_waiter(&self, wait_object: &WaitObject) {
        let mut waiters = self.waiters.lock();
        if !wait_object.on_list() {
            return;
        }

        assert_eq!(
            wait_object.wait_list(),
            self,
            "Wait object is not in the wait list."
        );

        let mut waiter = unsafe {
            // SAFETY: `wait_object` is on the `waiters` list.
            waiters.cursor_mut_from_ptr(wait_object)
        };

        unsafe {
            // SAFETY: We got the cursor from a valid wait object, which can't be null.
            self.notify_waiter_unchecked(&mut waiter);
        }
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
