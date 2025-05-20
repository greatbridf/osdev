pub trait UnlockableGuard {
    type Unlocked: UnlockedGuard<Guard = Self>;

    #[must_use = "The returned `UnlockedGuard` must be used to relock the lock."]
    fn unlock(self) -> Self::Unlocked;
}

/// # Safety
/// Implementors of this trait MUST ensure that the lock is correctly unlocked if
/// the lock is stateful and dropped accidentally.
pub unsafe trait UnlockedGuard: Send {
    type Guard: UnlockableGuard;

    #[must_use = "Throwing away the relocked guard is pointless."]
    fn relock(self) -> impl Future<Output = Self::Guard> + Send;
}
