use crate::ForceUnlockableGuard;

use super::{Mutex, Wait};
use core::{
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    sync::atomic::Ordering,
};

pub struct MutexGuard<'a, T, W>
where
    T: ?Sized,
    W: Wait,
{
    pub(super) lock: &'a Mutex<T, W>,
    pub(super) value: &'a mut T,
}

impl<T, W> Drop for MutexGuard<'_, T, W>
where
    T: ?Sized,
    W: Wait,
{
    fn drop(&mut self) {
        let locked = self.lock.locked.swap(false, Ordering::Release);
        debug_assert!(
            locked,
            "MutexGuard::drop(): unlock() called on an unlocked mutex.",
        );
        self.lock.wait.notify();
    }
}

impl<T, W> Deref for MutexGuard<'_, T, W>
where
    T: ?Sized,
    W: Wait,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<T, W> DerefMut for MutexGuard<'_, T, W>
where
    T: ?Sized,
    W: Wait,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
    }
}

impl<T, U, W> AsRef<U> for MutexGuard<'_, T, W>
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

impl<T, U, W> AsMut<U> for MutexGuard<'_, T, W>
where
    T: ?Sized + AsMut<U>,
    U: ?Sized,
    <Self as Deref>::Target: AsMut<U>,
    W: Wait,
{
    fn as_mut(&mut self) -> &mut U {
        self.deref_mut().as_mut()
    }
}

impl<T, W> ForceUnlockableGuard for MutexGuard<'_, T, W>
where
    T: ?Sized,
    W: Wait,
{
    unsafe fn force_unlock(&mut self) {
        let locked = self.lock.locked.swap(false, Ordering::Release);
        debug_assert!(
            locked,
            "MutexGuard::drop(): unlock() called on an unlocked mutex.",
        );
        self.lock.wait.notify();
    }

    unsafe fn force_relock(&mut self) {
        let _ = ManuallyDrop::new(if let Some(guard) = self.lock.try_lock() {
            guard
        } else {
            self.lock.lock_slow_path()
        });
    }
}
