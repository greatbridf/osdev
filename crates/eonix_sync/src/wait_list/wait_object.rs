use crate::Spin;
use core::{marker::PhantomPinned, pin::Pin, sync::atomic::AtomicBool, task::Waker};
use intrusive_collections::{intrusive_adapter, LinkedListAtomicLink, UnsafeRef};

intrusive_adapter!(
    pub WaitObjectAdapter = Pin<UnsafeRef<WaitObject>>:
    WaitObject { link: LinkedListAtomicLink }
);

pub struct WaitObject {
    pub(super) woken_up: AtomicBool,
    pub(super) waker: Spin<Option<Waker>>,
    link: LinkedListAtomicLink,
    _pinned: PhantomPinned,
}

impl WaitObject {
    pub const fn new() -> Self {
        Self {
            woken_up: AtomicBool::new(false),
            waker: Spin::new(None),
            link: LinkedListAtomicLink::new(),
            _pinned: PhantomPinned,
        }
    }

    pub fn on_list(&self) -> bool {
        self.link.is_linked()
    }
}
