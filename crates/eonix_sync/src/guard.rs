pub trait UnlockableGuard {
    type Unlocked: UnlockedGuard<Guard = Self>;

    #[must_use = "The returned `UnlockedGuard` must be used to relock the lock."]
    fn unlock(self) -> Self::Unlocked;
}

/// # Safety
/// Implementors of this trait MUST ensure that the lock is correctly unlocked if
/// the lock is stateful and dropped accidentally.
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

    fn do_unlocked(&mut self, f: impl FnOnce())
    where
        Self: Sized,
    {
        // SAFETY: We unlock the lock before calling the function and relock it after
        // calling the function. So we will end up with the lock being held again.
        unsafe {
            self.force_unlock();
            f();
            self.force_relock();
        }
    }
}
