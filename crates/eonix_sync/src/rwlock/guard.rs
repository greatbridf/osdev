use crate::{AsProof, AsProofMut, ForceUnlockableGuard, Proof, ProofMut};

use super::{RwLock, Wait};
use core::{
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    sync::atomic::Ordering,
};

pub struct RwLockWriteGuard<'a, T, W>
where
    T: ?Sized,
    W: Wait,
{
    pub(super) lock: &'a RwLock<T, W>,
    pub(super) value: &'a mut T,
}

pub struct RwLockReadGuard<'a, T, W>
where
    T: ?Sized,
    W: Wait,
{
    pub(super) lock: &'a RwLock<T, W>,
    pub(super) value: &'a T,
}

impl<T, W> Drop for RwLockWriteGuard<'_, T, W>
where
    T: ?Sized,
    W: Wait,
{
    fn drop(&mut self) {
        let old = self.lock.counter.swap(0, Ordering::Release);
        assert_eq!(
            old, -1,
            "RwLockWriteGuard::drop(): erroneous counter value: {}",
            old
        );
        self.lock.wait.write_notify();
    }
}

impl<T, W> Drop for RwLockReadGuard<'_, T, W>
where
    T: ?Sized,
    W: Wait,
{
    fn drop(&mut self) {
        match self.lock.counter.fetch_sub(1, Ordering::Release) {
            2.. => {}
            1 => self.lock.wait.read_notify(),
            val => unreachable!("RwLockReadGuard::drop(): erroneous counter value: {}", val),
        }
    }
}

impl<T, W> Deref for RwLockWriteGuard<'_, T, W>
where
    T: ?Sized,
    W: Wait,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<T, W> DerefMut for RwLockWriteGuard<'_, T, W>
where
    T: ?Sized,
    W: Wait,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
    }
}

impl<T, U, W> AsRef<U> for RwLockWriteGuard<'_, T, W>
where
    T: ?Sized,
    U: ?Sized,
    <Self as Deref>::Target: AsRef<U>,
    W: Wait,
{
    fn as_ref(&self) -> &U {
        self.deref().as_ref()
    }
}

impl<T, U, W> AsMut<U> for RwLockWriteGuard<'_, T, W>
where
    T: ?Sized,
    U: ?Sized,
    <Self as Deref>::Target: AsMut<U>,
    W: Wait,
{
    fn as_mut(&mut self) -> &mut U {
        self.deref_mut().as_mut()
    }
}

impl<T, W> Deref for RwLockReadGuard<'_, T, W>
where
    T: ?Sized,
    W: Wait,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<T, U, W> AsRef<U> for RwLockReadGuard<'_, T, W>
where
    T: ?Sized,
    U: ?Sized,
    <Self as Deref>::Target: AsRef<U>,
    W: Wait,
{
    fn as_ref(&self) -> &U {
        self.deref().as_ref()
    }
}

unsafe impl<'guard, 'pos, T, W> AsProof<'guard, 'pos, T> for RwLockWriteGuard<'guard, T, W>
where
    T: ?Sized,
    W: Wait,
{
    fn prove(&self) -> Proof<'pos, T> {
        unsafe { Proof::new(&raw const *self.value) }
    }
}

unsafe impl<'guard, 'pos, T, W> AsProofMut<'guard, 'pos, T> for RwLockWriteGuard<'guard, T, W>
where
    T: ?Sized,
    W: Wait,
{
    fn prove_mut(&self) -> ProofMut<'pos, T> {
        unsafe { ProofMut::new(&raw const *self.value as *mut _) }
    }
}

unsafe impl<'guard, 'pos, T, W> AsProof<'guard, 'pos, T> for RwLockReadGuard<'guard, T, W>
where
    T: ?Sized,
    W: Wait,
{
    fn prove(&self) -> Proof<'pos, T> {
        unsafe { Proof::new(&raw const *self.value) }
    }
}

impl<'a, T, W> ForceUnlockableGuard for RwLockReadGuard<'_, T, W>
where
    T: ?Sized,
    W: Wait,
{
    unsafe fn force_unlock(&mut self) {
        match self.lock.counter.fetch_sub(1, Ordering::Release) {
            2.. => {}
            1 => self.lock.wait.read_notify(),
            val => unreachable!("RwLockReadGuard::drop(): erroneous counter value: {}", val),
        }
    }

    unsafe fn force_relock(&mut self) {
        let _ = ManuallyDrop::new(if let Some(guard) = self.lock.try_read() {
            // Quick path
            guard
        } else {
            self.lock.read_slow_path()
        });
    }
}
