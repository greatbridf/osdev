use crate::{Lock, LockStrategy};
use core::{
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    ptr,
};

pub struct Guard<'a, T, S, L, const WRITE: bool = true>
where
    T: ?Sized,
    S: LockStrategy,
    L: LockStrategy,
{
    pub(crate) lock: &'a Lock<T, L>,
    pub(crate) strategy_data: &'a S::StrategyData,
    pub(crate) context: S::GuardContext,
}

pub struct UnlockedGuard<'a, T, S, L, const WRITE: bool = true>
where
    T: ?Sized,
    S: LockStrategy,
    L: LockStrategy,
{
    pub(crate) lock: &'a Lock<T, L>,
    pub(crate) strategy_data: &'a S::StrategyData,
    pub(crate) context: S::GuardContext,
}

impl<'a, T, S, L, const W: bool> Guard<'a, T, S, L, W>
where
    T: ?Sized,
    S: LockStrategy,
    L: LockStrategy,
{
    #[must_use = "The returned `UnlockedGuard` must be used to relock the lock."]
    pub fn unlock(mut self) -> UnlockedGuard<'a, T, S, L, W> {
        unsafe { S::do_temporary_unlock(&self.strategy_data, &mut self.context) }

        UnlockedGuard {
            lock: self.lock,
            strategy_data: self.strategy_data,
            context: {
                let me = ManuallyDrop::new(self);
                // SAFETY: We are using `ManuallyDrop` to prevent the destructor from running.
                unsafe { ptr::read(&me.context) }
            },
        }
    }

    /// # Safety
    /// This function is unsafe because it allows you to unlock the lock without
    /// dropping the guard. Using the guard after calling this function is
    /// undefined behavior.
    pub unsafe fn force_unlock(&mut self) {
        unsafe { S::do_temporary_unlock(&self.strategy_data, &mut self.context) }
    }

    /// # Safety
    /// Calling this function twice on a force unlocked guard will cause deadlocks.
    pub unsafe fn force_relock(&mut self) {
        unsafe { S::do_relock(&self.strategy_data, &mut self.context) }
    }
}

impl<'a, T, S, L, const W: bool> UnlockedGuard<'a, T, S, L, W>
where
    T: ?Sized,
    S: LockStrategy,
    L: LockStrategy,
{
    #[must_use = "Throwing away the relocked guard is pointless."]
    pub fn relock(mut self) -> Guard<'a, T, S, L, W> {
        unsafe { S::do_relock(&self.strategy_data, &mut self.context) }

        Guard {
            lock: self.lock,
            strategy_data: self.strategy_data,
            context: {
                let me = ManuallyDrop::new(self);
                // SAFETY: We are using `ManuallyDrop` to prevent the destructor from running.
                unsafe { ptr::read(&me.context) }
            },
        }
    }
}

impl<T, S, L, const W: bool> Deref for Guard<'_, T, S, L, W>
where
    T: ?Sized,
    S: LockStrategy,
    L: LockStrategy,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T, S, L> DerefMut for Guard<'_, T, S, L, true>
where
    T: ?Sized,
    S: LockStrategy,
    L: LockStrategy,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T, S, L, const WRITE: bool> AsRef<T> for Guard<'_, T, S, L, WRITE>
where
    T: ?Sized,
    S: LockStrategy,
    L: LockStrategy,
{
    fn as_ref(&self) -> &T {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T, S, L> AsMut<T> for Guard<'_, T, S, L, true>
where
    T: ?Sized,
    S: LockStrategy,
    L: LockStrategy,
{
    fn as_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T, S, L, const WRITE: bool> Drop for UnlockedGuard<'_, T, S, L, WRITE>
where
    T: ?Sized,
    S: LockStrategy,
    L: LockStrategy,
{
    fn drop(&mut self) {
        // SAFETY: If we are stubborn enough to drop the unlocked guard, relock it and
        //         then unlock it again to prevent anything weird from happening.
        unsafe {
            S::do_relock(&self.strategy_data, &mut self.context);
            S::do_unlock(&self.strategy_data, &mut self.context);
        }
    }
}

impl<T, S, L, const WRITE: bool> Drop for Guard<'_, T, S, L, WRITE>
where
    T: ?Sized,
    S: LockStrategy,
    L: LockStrategy,
{
    fn drop(&mut self) {
        unsafe { S::do_unlock(&self.strategy_data, &mut self.context) }
    }
}
