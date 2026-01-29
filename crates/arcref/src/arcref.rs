#[cfg(not(feature = "std"))]
use core::{
    borrow::Borrow,
    marker::{PhantomData, Unsize},
    mem::ManuallyDrop,
    ops::{Deref, DispatchFromDyn},
};

#[cfg(all(not(feature = "std"), feature = "alloc"))]
extern crate alloc;

#[cfg(all(not(feature = "std"), feature = "alloc"))]
use alloc::sync::Arc;

#[cfg(feature = "std")]
use std::{
    borrow::Borrow,
    marker::{PhantomData, Unsize},
    mem::ManuallyDrop,
    ops::{Deref, DispatchFromDyn},
    sync::Arc,
};

pub trait AsArcRef<T>
where
    T: ?Sized,
{
    /// Borrow the [`Arc`] and convert the reference into [`ArcRef`].
    fn aref(&self) -> ArcRef<'_, T>;
}

pub struct ArcRef<'a, T: ?Sized> {
    ptr: *const T,
    _phantom: PhantomData<&'a ()>,
}

unsafe impl<T: ?Sized + Send + Sync> Send for ArcRef<'_, T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for ArcRef<'_, T> {}

#[cfg(any(feature = "std", feature = "alloc"))]
impl<'a, T: ?Sized> ArcRef<'a, T> {
    pub fn new(arc: &'a Arc<T>) -> Self {
        Self {
            ptr: Arc::as_ptr(arc),
            _phantom: PhantomData,
        }
    }

    /// Create a new `ArcRef` from a raw pointer.
    ///
    /// # Safety
    /// The given pointer MUST be created by `Arc::as_ptr` or `Arc::into_raw`.
    /// The caller is responsible to ensure that the pointer is valid for the
    /// lifetime of the `ArcRef`.
    pub unsafe fn new_unchecked(arc_ptr: *const T) -> Self {
        Self {
            ptr: arc_ptr,
            _phantom: PhantomData,
        }
    }

    pub fn with_arc<Func, Out>(self, func: Func) -> Out
    where
        Func: FnOnce(&Arc<T>) -> Out,
    {
        func(&ManuallyDrop::new(unsafe { Arc::from_raw(self.ptr) }))
    }

    pub fn clone_arc(self) -> Arc<T> {
        self.with_arc(|arc| arc.clone())
    }

    pub fn ptr_eq_arc(self, other: &Arc<T>) -> bool {
        self.with_arc(|arc| Arc::ptr_eq(arc, other))
    }
}

#[cfg(all(not(feature = "std"), feature = "alloc"))]
impl<T> AsArcRef<T> for Arc<T>
where
    T: ?Sized,
{
    fn aref(&self) -> ArcRef<'_, T> {
        ArcRef::new(self)
    }
}

impl<T> AsRef<T> for ArcRef<'_, T>
where
    T: ?Sized,
{
    fn as_ref(&self) -> &T {
        self.deref()
    }
}

impl<T> Borrow<T> for ArcRef<'_, T>
where
    T: ?Sized,
{
    fn borrow(&self) -> &T {
        self.deref()
    }
}

impl<'a, T> Clone for ArcRef<'a, T>
where
    T: ?Sized,
{
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            _phantom: PhantomData,
        }
    }
}

impl<T> Copy for ArcRef<'_, T> where T: ?Sized {}

impl<T: ?Sized> Deref for ArcRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe {
            // SAFETY: `self.ptr` points to a valid `T` instance because it was
            //         created from a valid `Arc<T>`.
            self.ptr.as_ref().unwrap_unchecked()
        }
    }
}

impl<'a, T, U> DispatchFromDyn<ArcRef<'a, U>> for ArcRef<'a, T>
where
    T: ?Sized + Unsize<U>,
    U: ?Sized,
{
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_from_arc() {
        let data = Arc::new(42);
        let _arc_ref = ArcRef::new(&data);
    }

    #[test]
    fn deref() {
        let data = Arc::new(42);
        let arc_ref = ArcRef::new(&data);

        assert_eq!(*arc_ref, 42);
    }

    #[test]
    fn clone_into_arc() {
        let data = Arc::new(42);
        let arc_ref = ArcRef::new(&data);

        let cloned = arc_ref.clone_arc();

        assert_eq!(Arc::strong_count(&data), 2);
        assert_eq!(*cloned, 42);
    }

    #[test]
    fn dyn_compatible_receiver() {
        struct Data(u32);

        trait Trait {
            fn foo(self: ArcRef<Self>) -> u32;
        }

        impl Trait for Data {
            fn foo(self: ArcRef<Self>) -> u32 {
                self.0
            }
        }

        let data = Arc::new(Data(42));
        let arc_ref = ArcRef::new(&data);

        assert_eq!(arc_ref.foo(), 42);
    }

    #[test]
    fn clone_from_train_methods() {
        struct Data(u32);

        trait Trait {
            fn foo(&self) -> u32;

            fn clone_self(self: ArcRef<Self>) -> Arc<dyn Trait>;
        }

        impl Trait for Data {
            fn foo(&self) -> u32 {
                self.0
            }

            fn clone_self(self: ArcRef<Self>) -> Arc<dyn Trait> {
                self.clone_arc() as _
            }
        }

        let data = Arc::new(Data(42));
        let arc_ref = ArcRef::new(&data);

        let cloned = arc_ref.clone_self();

        assert_eq!(arc_ref.foo(), 42);
        assert_eq!(cloned.foo(), 42);
    }
}
