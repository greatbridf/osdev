use core::{marker::PhantomData, ptr::NonNull};

/// A proof of mutable access to a position in memory with lifetime `'pos`.
///
/// Note that this is just the proof of access, we aren't actually permitted
/// to mutate the memory location.
///
/// We can choose whether to check the validity of the proof or not at runtime.
pub struct ProofMut<'pos, T>
where
    T: ?Sized,
{
    pub(super) address: NonNull<T>,
    _phantom: PhantomData<&'pos ()>,
}

/// A proof of immutable access to a position in memory with lifetime `'pos`.
///
/// We can choose whether to check the validity of the proof or not at runtime.
pub struct Proof<'pos, T>
where
    T: ?Sized,
{
    pub(super) address: NonNull<T>,
    _phantom: PhantomData<&'pos ()>,
}

unsafe impl<T: ?Sized> Send for Proof<'_, T> {}
unsafe impl<T: ?Sized> Send for ProofMut<'_, T> {}

/// A trait for types that can be converted to a proof of mutable access.
///
/// This is used to prove that a mutable reference is valid for the lifetime `'pos`
/// through this object with lifetime `'guard`.
///
/// ## Safety
/// This trait is unsafe because it allows the caller to create a proof of access
/// to a memory location that may not be valid for the lifetime `'pos`. The implementer
/// must ensure that the access to the memory location is valid for the lifetime `'pos`
/// during the lifetime of the returned `Proof`. This is typically done by using a lock
/// or other synchronization mechanism to ensure that the memory location is not accessed
/// by others while the proof is being created.
pub unsafe trait AsProofMut<'guard, 'pos, T>: 'guard
where
    T: ?Sized,
{
    fn prove_mut(&self) -> ProofMut<'pos, T>
    where
        'guard: 'pos;
}

/// A trait for types that can be converted to a proof of immutable access.
///
/// This is used to prove that an immutable reference is valid for the lifetime `'pos`
/// through this object with lifetime `'guard`.
///
/// ## Safety
/// This trait is unsafe because it allows the caller to create a proof of access
/// to a memory location that may not be valid for the lifetime `'pos`. The implementer
/// must ensure that the access to the memory location is valid for the lifetime `'pos`
/// during the lifetime of the returned `Proof`. This is typically done by using a lock
/// or other synchronization mechanism to ensure that the memory location is not accessed
/// by others while the proof is being created.
pub unsafe trait AsProof<'guard, 'pos, T>: 'guard
where
    T: ?Sized,
{
    fn prove(&self) -> Proof<'pos, T>
    where
        'guard: 'pos;
}

impl<T> Proof<'_, T>
where
    T: ?Sized,
{
    /// # Safety
    /// The caller must ensure valid access for at least the lifetime `'pos`.
    pub const unsafe fn new(address: *const T) -> Self {
        Self {
            // SAFETY: The validity of the reference is guaranteed by the caller.
            address: unsafe { NonNull::new_unchecked(address as *mut _) },
            _phantom: PhantomData,
        }
    }
}

impl<T> ProofMut<'_, T>
where
    T: ?Sized,
{
    /// # Safety
    /// The caller must ensure valid access for at least the lifetime `'pos`.
    pub const unsafe fn new(address: *mut T) -> Self {
        Self {
            // SAFETY: The validity of the reference is guaranteed by the caller.
            address: unsafe { NonNull::new_unchecked(address as *mut _) },
            _phantom: PhantomData,
        }
    }
}

/// Proof of mutable access to a position in memory can be duplicated.
impl<T> Copy for ProofMut<'_, T> where T: ?Sized {}

/// Proof of mutable access to a position in memory can be duplicated.
impl<T> Clone for ProofMut<'_, T>
where
    T: ?Sized,
{
    fn clone(&self) -> Self {
        Self {
            address: self.address,
            _phantom: self._phantom,
        }
    }
}

/// Proof of immutable access to a position in memory can be duplicated.
impl<T> Copy for Proof<'_, T> where T: ?Sized {}

/// Proof of immutable access to a position in memory can be duplicated.
impl<T> Clone for Proof<'_, T>
where
    T: ?Sized,
{
    fn clone(&self) -> Self {
        Self {
            address: self.address,
            _phantom: self._phantom,
        }
    }
}

/// SAFETY: The reference is valid for the lifetime `'guard`. So the access must be
/// valid for the lifetime `'pos` that is shorter than `'guard`.
unsafe impl<'guard, 'pos, T> AsProofMut<'guard, 'pos, T> for &'guard mut T
where
    T: ?Sized,
{
    fn prove_mut(&self) -> ProofMut<'pos, T>
    where
        'guard: 'pos,
    {
        ProofMut {
            // SAFETY: The validity of the reference is guaranteed by the borrow checker.
            address: unsafe { NonNull::new_unchecked(&raw const **self as *mut _) },
            _phantom: PhantomData,
        }
    }
}

/// SAFETY: The reference is valid for the lifetime `'guard`. So the access must be
/// valid for the lifetime `'pos` that is shorter than `'guard`.
unsafe impl<'guard, 'pos, T> AsProof<'guard, 'pos, T> for &'guard T
where
    T: ?Sized,
{
    fn prove(&self) -> Proof<'pos, T>
    where
        'guard: 'pos,
    {
        Proof {
            address: unsafe { NonNull::new_unchecked(&raw const **self as *mut _) },
            _phantom: PhantomData,
        }
    }
}

/// SAFETY: The reference is valid for the lifetime `'guard`. So the access must be
/// valid for the lifetime `'pos` that is shorter than `'guard`.
unsafe impl<'guard, 'pos, T> AsProof<'guard, 'pos, T> for &'guard mut T
where
    T: ?Sized,
{
    fn prove(&self) -> Proof<'pos, T>
    where
        'guard: 'pos,
    {
        Proof {
            address: unsafe { NonNull::new_unchecked(&raw const **self as *mut _) },
            _phantom: PhantomData,
        }
    }
}
