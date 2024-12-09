use core::{cell::UnsafeCell, marker::PhantomData};

use super::{lock::Guard, strategy::LockStrategy};

pub struct RefMutPosition<'pos, T: ?Sized> {
    address: *const T,
    _phantom: PhantomData<&'pos ()>,
}

pub struct RefPosition<'pos, T: ?Sized> {
    address: *const T,
    _phantom: PhantomData<&'pos ()>,
}

pub trait AsRefMutPosition<'guard, 'pos, T: ?Sized>: 'guard {
    fn as_pos_mut(&self) -> RefMutPosition<'pos, T>
    where
        'guard: 'pos;
}

pub trait AsRefPosition<'guard, 'pos, T: ?Sized>: 'guard {
    fn as_pos(&self) -> RefPosition<'pos, T>
    where
        'guard: 'pos;
}

unsafe impl<T: Sized + Send, U: ?Sized> Send for Locked<T, U> {}
unsafe impl<T: Sized + Send + Sync, U: ?Sized> Sync for Locked<T, U> {}
pub struct Locked<T: Sized, U: ?Sized> {
    inner: UnsafeCell<T>,
    guard: *const U,
}

impl<T: ?Sized> Copy for RefPosition<'_, T> {}
impl<T: ?Sized> Clone for RefPosition<'_, T> {
    fn clone(&self) -> Self {
        Self {
            address: self.address,
            _phantom: self._phantom,
        }
    }
}

impl<T: ?Sized> Copy for RefMutPosition<'_, T> {}
impl<T: ?Sized> Clone for RefMutPosition<'_, T> {
    fn clone(&self) -> Self {
        Self {
            address: self.address,
            _phantom: self._phantom,
        }
    }
}

impl<'lock, 'pos, T: ?Sized> AsRefMutPosition<'lock, 'pos, T> for &'lock mut T {
    fn as_pos_mut(&self) -> RefMutPosition<'pos, T>
    where
        'lock: 'pos,
    {
        RefMutPosition {
            address: *self as *const T,
            _phantom: PhantomData,
        }
    }
}

impl<'lock, 'pos, T, S> AsRefMutPosition<'lock, 'pos, T> for Guard<'lock, T, S, true>
where
    T: ?Sized,
    S: LockStrategy + 'lock,
{
    fn as_pos_mut(&self) -> RefMutPosition<'pos, T>
    where
        'lock: 'pos,
    {
        RefMutPosition {
            address: &raw const **self,
            _phantom: PhantomData,
        }
    }
}

impl<'lock, 'pos, T: ?Sized> AsRefPosition<'lock, 'pos, T> for &'lock T {
    fn as_pos(&self) -> RefPosition<'pos, T>
    where
        'lock: 'pos,
    {
        RefPosition {
            address: *self as *const T,
            _phantom: PhantomData,
        }
    }
}

impl<'lock, 'pos, T: ?Sized> AsRefPosition<'lock, 'pos, T> for &'lock mut T {
    fn as_pos(&self) -> RefPosition<'pos, T>
    where
        'lock: 'pos,
    {
        RefPosition {
            address: *self as *const T,
            _phantom: PhantomData,
        }
    }
}

impl<'lock, 'pos, T, S, const B: bool> AsRefPosition<'lock, 'pos, T> for Guard<'lock, T, S, B>
where
    T: ?Sized,
    S: LockStrategy + 'lock,
{
    fn as_pos(&self) -> RefPosition<'pos, T>
    where
        'lock: 'pos,
    {
        RefPosition {
            address: &raw const **self,
            _phantom: PhantomData,
        }
    }
}

impl<T: Sized, U: ?Sized> core::fmt::Debug for Locked<T, U> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Locked")
            .field("value", &self.inner)
            .field("guard", &self.guard)
            .finish()
    }
}

impl<T: Sized + Sync, U: ?Sized> Locked<T, U> {
    pub fn new(value: T, guard: *const U) -> Self {
        Self {
            inner: UnsafeCell::new(value),
            guard,
        }
    }

    pub fn access<'lt>(&'lt self, guard: RefPosition<'lt, U>) -> &'lt T {
        assert_eq!(self.guard, guard.address, "Locked: Wrong guard");
        // SAFETY: The guard protects the shared access to the inner value.
        unsafe { self.inner.get().as_ref() }.unwrap()
    }

    pub fn access_mut<'lt>(&'lt self, guard: RefMutPosition<'lt, U>) -> &'lt mut T {
        assert_eq!(self.guard, guard.address, "Locked: Wrong guard");
        // SAFETY: The guard protects the exclusive access to the inner value.
        unsafe { self.inner.get().as_mut() }.unwrap()
    }
}
