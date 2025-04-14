use super::{wait_object::WaitObject, WaitList};
use core::{
    cell::UnsafeCell,
    pin::Pin,
    sync::atomic::Ordering,
    task::{Context, Poll, Waker},
};
use intrusive_collections::UnsafeRef;

pub struct Prepare<'a> {
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

impl<'a> Prepare<'a> {
    pub const fn new(wait_list: &'a WaitList) -> Self {
        Self {
            wait_list,
            wait_object: UnsafeCell::new(WaitObject::new()),
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
        // SAFETY: We only touch `state`, which is `Unpin`.
        unsafe {
            let this = self.get_unchecked_mut();
            this.state = state;
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
                let mut waiters = wait_list.waiters.lock();
                waiters.push_back(wait_object_ref);

                if let Some(waker) = waker.cloned() {
                    let old_waker = wait_object.waker.lock().replace(waker);
                    assert!(old_waker.is_none(), "Waker already set");
                    *state = State::WakerSet;
                } else {
                    *state = State::OnList;
                }

                return false;
            }
            // We are already on the wait list, so we can just set the waker.
            State::OnList => {
                // If we are already woken up, we can just return.
                if wait_object.woken_up.load(Ordering::Acquire) {
                    *state = State::WokenUp;
                    return true;
                }

                if let Some(waker) = waker {
                    // Lock the waker and check if it is already set.
                    let mut waker_lock = wait_object.waker.lock();
                    if wait_object.woken_up.load(Ordering::Acquire) {
                        *state = State::WokenUp;
                        return true;
                    }

                    let old_waker = waker_lock.replace(waker.clone());
                    assert!(old_waker.is_none(), "Waker already set");
                    *state = State::WakerSet;
                }

                return false;
            }
            _ => unreachable!("Invalid state."),
        }
    }

    pub fn add_to_wait_list(self: Pin<&mut Self>) {
        self.do_add_to_wait_list(None);
    }
}

impl Future for Prepare<'_> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.state {
            State::Init | State::OnList => {
                if self.as_mut().do_add_to_wait_list(Some(cx.waker())) {
                    // Make sure we're off the wait list.
                    while self.wait_object().on_list() {}
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            }
            State::WakerSet => {
                if !self.as_ref().wait_object().woken_up.load(Ordering::Acquire) {
                    // If we read `woken_up == false`, we can guarantee that we have a spurious
                    // wakeup. In this case, we MUST be still on the wait list, so no more
                    // actions are required.
                    Poll::Pending
                } else {
                    // Make sure we're off the wait list.
                    while self.wait_object().on_list() {}

                    self.set_state(State::WokenUp);
                    Poll::Ready(())
                }
            }
            State::WokenUp => Poll::Ready(()),
        }
    }
}

impl Drop for Prepare<'_> {
    fn drop(&mut self) {
        match self.state {
            State::Init | State::WokenUp => {}
            State::OnList | State::WakerSet => {
                let wait_object = self.wait_object();
                if wait_object.woken_up.load(Ordering::Acquire) {
                    // We've woken up by someone. It won't be long before they
                    // remove us from the list. So spin until we are off the list.
                    // And we're done.
                    while wait_object.on_list() {}
                } else {
                    // Lock the list and try again.
                    let mut waiters = self.wait_list.waiters.lock();

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
    }
}
