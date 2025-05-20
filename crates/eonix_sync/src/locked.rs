mod proof;

use core::{cell::UnsafeCell, fmt, ptr::NonNull};

pub use proof::{AsProof, AsProofMut, Proof, ProofMut};

/// A lock to protect a value of type `T` using the proof of access to some
/// value of type `U`.
pub struct Locked<T, U>
where
    U: ?Sized,
{
    inner: UnsafeCell<T>,
    #[cfg(not(feature = "no_check_locked"))]
    guard: NonNull<U>,
    #[cfg(feature = "no_check_locked")]
    _phantom: core::marker::PhantomData<NonNull<U>>,
}

/// SAFETY: The `Locked` type is safe to send across threads as long as
/// the inner type `T` is `Send`. The `guard` pointer is not used to access
/// the inner value, so no constraints are needed on it.
unsafe impl<T, U> Send for Locked<T, U>
where
    T: Send,
    U: ?Sized,
{
}

/// SAFETY: The `Locked` type is safe to share across threads as long as
/// the inner type `T` is `Send` and `Sync`. The `guard` pointer is not used
/// to access the inner value, so no constraints are needed on it.
unsafe impl<T, U> Sync for Locked<T, U>
where
    T: Send + Sync,
    U: ?Sized,
{
}

impl<T, U> Locked<T, U>
where
    U: ?Sized,
{
    pub const fn new(value: T, guard: &U) -> Self {
        Self {
            inner: UnsafeCell::new(value),
            #[cfg(not(feature = "no_check_locked"))]
            // SAFETY: The validity of address is guaranteed by the borrow checker.
            guard: unsafe { NonNull::new_unchecked(&raw const *guard as *mut U) },
            #[cfg(feature = "no_check_locked")]
            guard: core::marker::PhantomData,
        }
    }
}

impl<T, U> Locked<T, U>
where
    T: Send + Sync,
    U: ?Sized,
{
    pub fn access<'a, 'b>(&'a self, _guard: Proof<'b, U>) -> &'a T
    where
        'b: 'a,
    {
        #[cfg(not(feature = "no_check_locked"))]
        assert_eq!(self.guard, _guard.address, "Locked::access(): Wrong guard");
        // SAFETY: The guard protects the shared access to the inner value.
        unsafe { self.inner.get().as_ref().unwrap() }
    }

    pub fn access_mut<'a, 'b>(&'a self, _guard: ProofMut<'b, U>) -> &'a mut T
    where
        'b: 'a,
    {
        #[cfg(not(feature = "no_check_locked"))]
        assert_eq!(
            self.guard, _guard.address,
            "Locked::access_mut(): Wrong guard"
        );
        // SAFETY: The guard protects the exclusive access to the inner value.
        unsafe { self.inner.get().as_mut().unwrap() }
    }
}

impl<T, U> fmt::Debug for Locked<T, U>
where
    U: ?Sized,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Locked")
            .field("value", &self.inner)
            .field("guard", &self.guard)
            .finish()
    }
}
