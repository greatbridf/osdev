use super::RwLock;
use crate::{AsProof, AsProofMut, Proof, ProofMut, UnlockableGuard, UnlockedGuard};
use core::ops::{Deref, DerefMut};

pub struct RwLockWriteGuard<'a, T>
where
    T: ?Sized,
{
    pub(super) lock: &'a RwLock<T>,
    pub(super) value: &'a mut T,
}

pub struct RwLockReadGuard<'a, T>
where
    T: ?Sized,
{
    pub(super) lock: &'a RwLock<T>,
    pub(super) value: &'a T,
}

pub struct UnlockedRwLockReadGuard<'a, T>(&'a RwLock<T>)
where
    T: ?Sized;

pub struct UnlockedRwLockWriteGuard<'a, T>(&'a RwLock<T>)
where
    T: ?Sized;

impl<T> Drop for RwLockWriteGuard<'_, T>
where
    T: ?Sized,
{
    fn drop(&mut self) {
        unsafe {
            // SAFETY: We are dropping the guard.
            self.lock.write_unlock();
        }
    }
}

impl<T> Drop for RwLockReadGuard<'_, T>
where
    T: ?Sized,
{
    fn drop(&mut self) {
        unsafe {
            // SAFETY: We are dropping the guard.
            self.lock.read_unlock();
        }
    }
}

impl<T> Deref for RwLockWriteGuard<'_, T>
where
    T: ?Sized,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<T> DerefMut for RwLockWriteGuard<'_, T>
where
    T: ?Sized,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
    }
}

impl<T, U> AsRef<U> for RwLockWriteGuard<'_, T>
where
    T: ?Sized,
    U: ?Sized,
    <Self as Deref>::Target: AsRef<U>,
{
    fn as_ref(&self) -> &U {
        self.deref().as_ref()
    }
}

impl<T, U> AsMut<U> for RwLockWriteGuard<'_, T>
where
    T: ?Sized,
    U: ?Sized,
    <Self as Deref>::Target: AsMut<U>,
{
    fn as_mut(&mut self) -> &mut U {
        self.deref_mut().as_mut()
    }
}

impl<T> Deref for RwLockReadGuard<'_, T>
where
    T: ?Sized,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<T, U> AsRef<U> for RwLockReadGuard<'_, T>
where
    T: ?Sized,
    U: ?Sized,
    <Self as Deref>::Target: AsRef<U>,
{
    fn as_ref(&self) -> &U {
        self.deref().as_ref()
    }
}

unsafe impl<'guard, 'pos, T> AsProof<'guard, 'pos, T> for RwLockWriteGuard<'guard, T>
where
    T: ?Sized,
{
    fn prove(&self) -> Proof<'pos, T> {
        unsafe { Proof::new(&raw const *self.value) }
    }
}

unsafe impl<'guard, 'pos, T> AsProofMut<'guard, 'pos, T> for RwLockWriteGuard<'guard, T>
where
    T: ?Sized,
{
    fn prove_mut(&self) -> ProofMut<'pos, T> {
        unsafe { ProofMut::new(&raw const *self.value as *mut _) }
    }
}

unsafe impl<'guard, 'pos, T> AsProof<'guard, 'pos, T> for RwLockReadGuard<'guard, T>
where
    T: ?Sized,
{
    fn prove(&self) -> Proof<'pos, T> {
        unsafe { Proof::new(&raw const *self.value) }
    }
}

impl<'a, T> UnlockableGuard for RwLockReadGuard<'a, T>
where
    T: ?Sized + Send + Sync,
{
    type Unlocked = UnlockedRwLockReadGuard<'a, T>;

    fn unlock(self) -> Self::Unlocked {
        // The lock will be unlocked when the guard is dropped.
        UnlockedRwLockReadGuard(self.lock)
    }
}

// SAFETY: `UnlockedRwLockReadGuard` is stateless.
unsafe impl<'a, T> UnlockedGuard for UnlockedRwLockReadGuard<'a, T>
where
    T: ?Sized + Send + Sync,
{
    type Guard = RwLockReadGuard<'a, T>;

    async fn relock(self) -> Self::Guard {
        let Self(lock) = self;
        lock.read().await
    }
}

impl<'a, T> UnlockableGuard for RwLockWriteGuard<'a, T>
where
    T: ?Sized + Send + Sync,
{
    type Unlocked = UnlockedRwLockWriteGuard<'a, T>;

    fn unlock(self) -> Self::Unlocked {
        // The lock will be unlocked when the guard is dropped.
        UnlockedRwLockWriteGuard(self.lock)
    }
}

// SAFETY: `UnlockedRwLockWriteGuard` is stateless.
unsafe impl<'a, T> UnlockedGuard for UnlockedRwLockWriteGuard<'a, T>
where
    T: ?Sized + Send + Sync,
{
    type Guard = RwLockWriteGuard<'a, T>;

    async fn relock(self) -> Self::Guard {
        let Self(lock) = self;
        lock.write().await
    }
}
