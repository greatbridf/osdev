use super::Mutex;
use crate::{UnlockableGuard, UnlockedGuard};
use core::{
    ops::{Deref, DerefMut},
    sync::atomic::Ordering,
};

pub struct MutexGuard<'a, T>
where
    T: ?Sized,
{
    pub(super) lock: &'a Mutex<T>,
    pub(super) value: &'a mut T,
}

pub struct UnlockedMutexGuard<'a, T>(&'a Mutex<T>)
where
    T: ?Sized;

impl<T> Drop for MutexGuard<'_, T>
where
    T: ?Sized,
{
    fn drop(&mut self) {
        let locked = self.lock.locked.swap(false, Ordering::Release);
        debug_assert!(
            locked,
            "MutexGuard::drop(): unlock() called on an unlocked mutex.",
        );
        self.lock.wait_list.notify_one();
    }
}

impl<T> Deref for MutexGuard<'_, T>
where
    T: ?Sized,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<T> DerefMut for MutexGuard<'_, T>
where
    T: ?Sized,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
    }
}

impl<T, U> AsRef<U> for MutexGuard<'_, T>
where
    T: ?Sized,
    U: ?Sized,
    <Self as Deref>::Target: AsRef<U>,
{
    fn as_ref(&self) -> &U {
        self.deref().as_ref()
    }
}

impl<T, U> AsMut<U> for MutexGuard<'_, T>
where
    T: ?Sized + AsMut<U>,
    U: ?Sized,
    <Self as Deref>::Target: AsMut<U>,
{
    fn as_mut(&mut self) -> &mut U {
        self.deref_mut().as_mut()
    }
}

impl<'a, T> UnlockableGuard for MutexGuard<'a, T>
where
    T: ?Sized + Send,
{
    type Unlocked = UnlockedMutexGuard<'a, T>;

    fn unlock(self) -> Self::Unlocked {
        // The lock will be unlocked when the guard is dropped.
        UnlockedMutexGuard(self.lock)
    }
}

unsafe impl<'a, T> UnlockedGuard for UnlockedMutexGuard<'a, T>
where
    T: ?Sized + Send,
{
    type Guard = MutexGuard<'a, T>;

    async fn relock(self) -> Self::Guard {
        let Self(lock) = self;
        lock.lock().await
    }
}
