#![no_std]

use alloc::sync::Arc;
use core::{marker::PhantomData, mem::ManuallyDrop, ops::Deref, ptr::NonNull};

extern crate alloc;

/// BorrowedArc is a wrapper around `Arc` that allows us to create an `Arc` from a raw pointer
/// that was created by `Arc::into_raw` when we are confident about that the original `Arc`
/// would be still valid during the whold lifetime of `BorrowedArc`.
///
/// # Example
///
/// ```should_run
/// use pointers::BorrowedArc;
/// use alloc::sync::Arc;
///
/// let arc = Arc::new(42);
/// let ptr = NonNull::new(Arc::into_raw(arc.clone())).unwrap();
///
/// // We know that the original `Arc` is still valid.
/// let borrowed_arc = unsafe { BorrowedArc::from_raw(ptr) };
///
/// let arc_reference: &Arc<i32> = &borrowed_arc;
/// assert_eq!(**arc_reference, 42);
/// ```
pub struct BorrowedArc<'a, T: ?Sized> {
    arc: ManuallyDrop<Arc<T>>,
    _phantom: PhantomData<&'a ()>,
}

impl<'a, T: ?Sized> BorrowedArc<'a, T> {
    /// # Safety
    /// If `ptr` is not a valid pointer to an `Arc<T>`, this will lead to undefined behavior.
    ///
    /// If the `Arc<T>` is dropped while `BorrowedArc` is still in use, this will lead
    /// to undefined behavior.
    pub unsafe fn from_raw(ptr: NonNull<T>) -> Self {
        Self {
            arc: ManuallyDrop::new(unsafe { Arc::from_raw(ptr.as_ptr()) }),
            _phantom: PhantomData,
        }
    }

    #[allow(dead_code)]
    pub fn new(ptr: &'a *const T) -> Self {
        assert!(!ptr.is_null());
        Self {
            arc: ManuallyDrop::new(unsafe { Arc::from_raw(*ptr) }),
            _phantom: PhantomData,
        }
    }

    pub fn borrow(&self) -> &'a T {
        let reference: &T = &self.arc;
        let ptr = reference as *const T;

        // SAFETY: `ptr` is a valid pointer to `T` because `reference` is a valid reference to `T`.
        // `ptr` is also guaranteed to be valid for the lifetime `'lt` because it is derived from
        // `self.arc` which is guaranteed to be valid for the lifetime `'lt`.
        unsafe { ptr.as_ref().unwrap() }
    }
}

impl<'a, T: ?Sized> Deref for BorrowedArc<'a, T> {
    type Target = Arc<T>;

    fn deref(&self) -> &Self::Target {
        &self.arc
    }
}

impl<'a, T: ?Sized> AsRef<Arc<T>> for BorrowedArc<'a, T> {
    fn as_ref(&self) -> &Arc<T> {
        &self.arc
    }
}
