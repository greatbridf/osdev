use super::{wait_object::WaitObject, WaitList};
use crate::SpinIrq as _;
use core::{
    cell::UnsafeCell,
    hint::spin_loop,
    pin::Pin,
    task::{Context, Poll, Waker},
};
use intrusive_collections::UnsafeRef;

pub struct WaitHandle<'a> {
    wait_list: &'a WaitList,
    wait_object: UnsafeCell<WaitObject>,
    state: State,
}

#[derive(Debug, PartialEq)]
enum State {
    Init,
    OnList,
    WakerSet,
    WokenUp,
}

struct PrepareSplit<'a> {
    wait_list: &'a WaitList,
    state: &'a mut State,
    wait_object: Pin<&'a WaitObject>,
}

// SAFETY: All access to `wait_object` is protected.
unsafe impl Sync for WaitHandle<'_> {}

impl<'a> WaitHandle<'a> {
    pub const fn new(wait_list: &'a WaitList) -> Self {
        Self {
            wait_list,
            wait_object: UnsafeCell::new(WaitObject::new(wait_list)),
            state: State::Init,
        }
    }

    fn wait_object(&self) -> &WaitObject {
        // SAFETY: We never get mutable references to a `WaitObject`.
        unsafe { &*self.wait_object.get() }
    }

    fn split_borrow(self: Pin<&mut Self>) -> PrepareSplit<'_> {
        unsafe {
            // SAFETY: `wait_list` and `state` is `Unpin`.
            let this = self.get_unchecked_mut();

            // SAFETY: `wait_object` is a field of a pinned struct.
            //         And we never get mutable references to a `WaitObject`.
            let wait_object = Pin::new_unchecked(&*this.wait_object.get());

            PrepareSplit {
                wait_list: this.wait_list,
                state: &mut this.state,
                wait_object,
            }
        }
    }

    fn set_state(self: Pin<&mut Self>, state: State) {
        unsafe {
            // SAFETY: We only touch `state`, which is `Unpin`.
            let this = self.get_unchecked_mut();
            this.state = state;
        }
    }

    fn wait_until_off_list(&self) {
        while self.wait_object().on_list() {
            spin_loop();
        }
    }

    /// # Returns
    /// Whether we've been woken up or not.
    fn do_add_to_wait_list(mut self: Pin<&mut Self>, waker: Option<&Waker>) -> bool {
        let PrepareSplit {
            wait_list,
            state,
            wait_object,
        } = self.as_mut().split_borrow();

        let wait_object_ref = unsafe {
            // SAFETY: `wait_object` is a valid reference to a `WaitObject` because we
            //         won't drop the wait object until the waiting thread will be woken
            //         up and make sure that it is not on the list.
            //
            // SAFETY: `wait_object` is a pinned reference to a `WaitObject`, so we can
            //         safely convert it to a `Pin<UnsafeRef<WaitObject>>`.
            Pin::new_unchecked(UnsafeRef::from_raw(&raw const *wait_object))
        };

        match *state {
            State::Init => {
                let mut waiters = wait_list.waiters.lock_irq();
                waiters.push_back(wait_object_ref);

                if let Some(waker) = waker.cloned() {
                    wait_object.save_waker(waker);
                    *state = State::WakerSet;
                } else {
                    *state = State::OnList;
                }

                return false;
            }
            // We are already on the wait list, so we can just set the waker.
            State::OnList => {
                // If we are already woken up, we can just return.
                if wait_object.woken_up() {
                    *state = State::WokenUp;
                    return true;
                }

                if let Some(waker) = waker {
                    // Lock the waker and check if it is already set.
                    let waker_set = wait_object.save_waker_if_not_woken_up(&waker);

                    if waker_set {
                        *state = State::WakerSet;
                    } else {
                        // We are already woken up, so we can just return.
                        *state = State::WokenUp;
                        return true;
                    }
                }

                return false;
            }
            _ => unreachable!("Invalid state."),
        }
    }

    pub fn add_to_wait_list(self: Pin<&mut Self>) {
        self.do_add_to_wait_list(None);
    }

    /// # Safety
    /// The caller MUST guarantee that the last use of the returned function
    /// is before `self` is dropped. Otherwise the value referred to in this
    /// function will be dangling and will cause undefined behavior.
    pub unsafe fn get_waker_function(self: Pin<&Self>) -> impl Fn() + Send + Sync + 'static {
        let wait_list: &WaitList = unsafe {
            // SAFETY: The caller guarantees that the last use of returned function
            //         is before `self` is dropped.
            &*(self.wait_list as *const _)
        };

        let wait_object = unsafe {
            // SAFETY: The caller guarantees that the last use of returned function
            //         is before `self` is dropped.
            &*self.wait_object.get()
        };

        move || {
            wait_list.notify_waiter(wait_object);
        }
    }
}

impl Future for WaitHandle<'_> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.state {
            State::Init | State::OnList => {
                if self.as_mut().do_add_to_wait_list(Some(cx.waker())) {
                    self.wait_until_off_list();
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            }
            State::WakerSet => {
                if !self.as_ref().wait_object().woken_up() {
                    // If we read `woken_up == false`, we can guarantee that we have a spurious
                    // wakeup. In this case, we MUST be still on the wait list, so no more
                    // actions are required.
                    Poll::Pending
                } else {
                    self.wait_until_off_list();
                    self.set_state(State::WokenUp);
                    Poll::Ready(())
                }
            }
            State::WokenUp => Poll::Ready(()),
        }
    }
}

impl Drop for WaitHandle<'_> {
    fn drop(&mut self) {
        if matches!(self.state, State::Init | State::WokenUp) {
            return;
        }

        let wait_object = self.wait_object();
        if wait_object.woken_up() {
            // We've woken up by someone. It won't be long before they
            // remove us from the list. So spin until we are off the list.
            // And we're done.
            self.wait_until_off_list();
        } else {
            // Lock the list and try again.
            let mut waiters = self.wait_list.waiters.lock_irq();

            if wait_object.on_list() {
                let mut cursor = unsafe {
                    // SAFETY: The list is locked so no one could be polling nodes
                    //         off while we are trying to remove it.
                    waiters.cursor_mut_from_ptr(wait_object)
                };
                assert!(cursor.remove().is_some());
            }
        }
    }
}
