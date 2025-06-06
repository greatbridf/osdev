use super::WaitList;
use crate::SpinIrq as _;
use core::{
    cell::UnsafeCell,
    marker::PhantomPinned,
    pin::Pin,
    ptr::null_mut,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
    task::Waker,
};
use eonix_spin::Spin;
use intrusive_collections::{intrusive_adapter, LinkedListAtomicLink, UnsafeRef};

intrusive_adapter!(
    pub WaitObjectAdapter = Pin<UnsafeRef<WaitObject>>:
    WaitObject { link: LinkedListAtomicLink }
);

pub struct WaitObject {
    woken_up: AtomicBool,
    /// Separation of the field `waker` from its lock is basically due to the
    /// consideration of space. We hope that the object can fit into a cacheline
    /// and `woken_up` takes only 1 byte where the rest 7 bytes can accomodate 1
    /// extra byte required for a spinlock.
    waker_lock: Spin<()>,
    waker: UnsafeCell<Option<Waker>>,
    wait_list: AtomicPtr<WaitList>,
    link: LinkedListAtomicLink,
    _pinned: PhantomPinned,
}

// SAFETY: `WaitObject` is `Sync` because we sync the `waker` access with a spinlock.
unsafe impl Sync for WaitObject {}

impl WaitObject {
    pub const fn new(wait_list: &WaitList) -> Self {
        Self {
            woken_up: AtomicBool::new(false),
            waker_lock: Spin::new(()),
            waker: UnsafeCell::new(None),
            wait_list: AtomicPtr::new(wait_list as *const _ as *mut _),
            link: LinkedListAtomicLink::new(),
            _pinned: PhantomPinned,
        }
    }

    pub fn save_waker(&self, waker: Waker) {
        let _lock = self.waker_lock.lock_irq();
        unsafe {
            // SAFETY: We're holding the waker lock.
            let old_waker = (*self.waker.get()).replace(waker);
            assert!(old_waker.is_none(), "Waker already set.");
        }
    }

    /// Save the waker if the wait object was not woken up atomically.
    ///
    /// # Returns
    /// Whether the waker was saved.
    pub fn save_waker_if_not_woken_up(&self, waker: &Waker) -> bool {
        let _lock = self.waker_lock.lock_irq();
        if self.woken_up() {
            return false;
        }

        unsafe {
            // SAFETY: We're holding the waker lock.
            let old_waker = (*self.waker.get()).replace(waker.clone());
            assert!(old_waker.is_none(), "Waker already set.");
        }

        true
    }

    pub fn take_waker(&self) -> Option<Waker> {
        let _lock = self.waker_lock.lock_irq();
        unsafe {
            // SAFETY: We're holding the waker lock.
            self.waker.get().as_mut().unwrap().take()
        }
    }

    /// Check whether someone had woken up the wait object.
    ///
    /// Does an `Acquire` operation.
    pub fn woken_up(&self) -> bool {
        self.woken_up.load(Ordering::Acquire)
    }

    /// Set the wait object as woken up.
    ///
    /// Does a `Release` operation.
    pub fn set_woken_up(&self) {
        self.woken_up.store(true, Ordering::Release);
    }

    pub fn wait_list(&self) -> *const WaitList {
        self.wait_list.load(Ordering::Acquire)
    }

    pub fn clear_wait_list(&self) {
        self.wait_list.store(null_mut(), Ordering::Release);
    }

    pub fn on_list(&self) -> bool {
        !self.wait_list.load(Ordering::Acquire).is_null()
    }
}
