use super::{Relax, Spin, SpinRelax};
use crate::{marker::NotSend, UnlockableGuard, UnlockedGuard};
use core::{
    marker::PhantomData,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
};

pub struct SpinGuard<'a, T, R = SpinRelax>
where
    T: ?Sized,
{
    pub(super) lock: &'a Spin<T, R>,
    pub(super) value: &'a mut T,
    /// We don't want this to be `Send` because we don't want to allow the guard to be
    /// transferred to another thread since we have disabled the preemption on the local cpu.
    pub(super) _not_send: PhantomData<NotSend>,
}

pub struct UnlockedSpinGuard<'a, T, R>(&'a Spin<T, R>)
where
    T: ?Sized;

// SAFETY: As long as the value protected by the lock is able to be shared between threads,
//         we can access the guard from multiple threads.
unsafe impl<T, R> Sync for SpinGuard<'_, T, R> where T: ?Sized + Sync {}

impl<T, R> Drop for SpinGuard<'_, T, R>
where
    T: ?Sized,
{
    fn drop(&mut self) {
        unsafe {
            // SAFETY: We are dropping the guard, so we are not holding the lock anymore.
            self.lock.do_unlock();
        }
    }
}

impl<T, R> Deref for SpinGuard<'_, T, R>
where
    T: ?Sized,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: We are holding the lock, so we can safely access the value.
        self.value
    }
}

impl<T, R> DerefMut for SpinGuard<'_, T, R>
where
    T: ?Sized,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: We are holding the lock, so we can safely access the value.
        self.value
    }
}

impl<T, U, R> AsRef<U> for SpinGuard<'_, T, R>
where
    T: ?Sized,
    U: ?Sized,
    <Self as Deref>::Target: AsRef<U>,
{
    fn as_ref(&self) -> &U {
        self.deref().as_ref()
    }
}

impl<T, U, R> AsMut<U> for SpinGuard<'_, T, R>
where
    T: ?Sized,
    U: ?Sized,
    <Self as Deref>::Target: AsMut<U>,
{
    fn as_mut(&mut self) -> &mut U {
        self.deref_mut().as_mut()
    }
}

impl<'a, T, R> UnlockableGuard for SpinGuard<'a, T, R>
where
    T: ?Sized + Send,
    R: Relax,
{
    type Unlocked = UnlockedSpinGuard<'a, T, R>;

    fn unlock(self) -> Self::Unlocked {
        let me = ManuallyDrop::new(self);
        unsafe {
            // SAFETY: No access is possible after unlocking.
            me.lock.do_unlock();
        }

        UnlockedSpinGuard(me.lock)
    }
}

// SAFETY: The guard is stateless so no more process needed.
unsafe impl<'a, T, R> UnlockedGuard for UnlockedSpinGuard<'a, T, R>
where
    T: ?Sized + Send,
    R: Relax,
{
    type Guard = SpinGuard<'a, T, R>;

    async fn relock(self) -> Self::Guard {
        let Self(lock) = self;
        lock.lock()
    }
}
