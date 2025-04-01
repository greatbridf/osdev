#![cfg_attr(feature = "no_std", no_std)]

#[cfg(feature = "no_std")]
use core::{
    cell::UnsafeCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};

#[cfg(not(feature = "no_std"))]
use std::{
    cell::UnsafeCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let cell = AtomicUniqueRefCell::new(42);
        let mut ref_cell = cell.borrow();
        assert_eq!(*ref_cell, 42);
        *ref_cell = 43;
        assert_eq!(*ref_cell, 43);
    }
}

/// `AtomicUniqueRefCell` implements `Send` and `Sync` if `T` is `Send`.
/// The following code will not compile if `T` is not `Send`.
///
/// ```compile_fail
/// use atomic_unique_refcell::AtomicUniqueRefCell;
///
/// struct NotSend {
///     data: *mut (),
/// }
///
/// struct Test {
///     data: AtomicUniqueRefCell<NotSend>,
/// }
///
/// trait TestTrait: Send + Sync {}
///
/// impl TestTrait for Test {}
/// ```
pub struct AtomicUniqueRefCell<T: ?Sized> {
    count: AtomicBool,
    inner: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for AtomicUniqueRefCell<T> {}
unsafe impl<T: ?Sized + Send> Sync for AtomicUniqueRefCell<T> {}

pub struct Ref<'a, T: ?Sized> {
    inner: &'a AtomicUniqueRefCell<T>,
    _marker: PhantomData<UnsafeCell<T>>,
}

impl<T> AtomicUniqueRefCell<T> {
    pub fn new(value: T) -> Self {
        Self {
            count: AtomicBool::new(false),
            inner: UnsafeCell::new(value),
        }
    }
}

impl<T: ?Sized> AtomicUniqueRefCell<T> {
    pub fn borrow(&self) -> Ref<'_, T> {
        if self.count.swap(true, Ordering::Acquire) {
            panic!("Already borrowed");
        }

        Ref {
            inner: self,
            _marker: PhantomData,
        }
    }
}

impl<T: ?Sized> Deref for Ref<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.inner.inner.get() }
    }
}

impl<T: ?Sized> DerefMut for Ref<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.inner.inner.get() }
    }
}

impl<T: ?Sized> Drop for Ref<'_, T> {
    fn drop(&mut self) {
        self.inner.count.swap(false, Ordering::Release);
    }
}
