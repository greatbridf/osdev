use crate::{Lock, LockStrategy};
use core::ops::{Deref, DerefMut};

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

pub trait UnlockableGuard {
    type Unlocked: UnlockedGuard<Guard = Self>;

    #[must_use = "The returned `UnlockedGuard` must be used to relock the lock."]
    fn unlock(self) -> Self::Unlocked;
}

/// # Safety
/// Implementors of this trait MUST ensure that the lock is correctly unlocked if
/// dropped accidentally.
pub unsafe trait UnlockedGuard {
    type Guard: UnlockableGuard;

    #[must_use = "Throwing away the relocked guard is pointless."]
    fn relock(self) -> Self::Guard;
}

pub trait ForceUnlockableGuard {
    /// # Safety
    /// This function is unsafe because it allows you to unlock the lock without
    /// dropping the guard. Using the guard after calling this function is
    /// undefined behavior.
    unsafe fn force_unlock(&mut self);

    /// # Safety
    /// Calling this function twice on a force unlocked guard will cause deadlocks.
    unsafe fn force_relock(&mut self);
}

impl<'a, T, S, L, const W: bool> ForceUnlockableGuard for Guard<'a, T, S, L, W>
where
    S: LockStrategy,
    L: LockStrategy,
{
    unsafe fn force_unlock(&mut self) {
        unsafe { S::do_temporary_unlock(&self.strategy_data, &mut self.context) }
    }

    unsafe fn force_relock(&mut self) {
        unsafe { S::do_relock(&self.strategy_data, &mut self.context) }
    }
}
