use super::{
    ContextUnlock, DisablePreemption, Relax, Spin, SpinContext, SpinRelax, UnlockedContext,
};
use core::{
    marker::PhantomData,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
};
use eonix_sync_base::{NotSend, UnlockableGuard, UnlockedGuard};

pub struct SpinGuard<'a, T, C = DisablePreemption, R = SpinRelax>
where
    T: ?Sized,
    C: SpinContext,
{
    lock: &'a Spin<T, R>,
    value: &'a mut T,
    context: Option<C>,
    /// We don't want this to be `Send` because we don't want to allow the guard to be
    /// transferred to another thread since we have disabled the preemption on the local cpu.
    _not_send: PhantomData<NotSend>,
}

pub struct UnlockedSpinGuard<'a, T, C, R>(&'a Spin<T, R>, C::Unlocked)
where
    T: ?Sized,
    C: ContextUnlock;

// SAFETY: As long as the value protected by the lock is able to be shared between threads,
//         we can access the guard from multiple threads.
unsafe impl<T, C, R> Sync for SpinGuard<'_, T, C, R>
where
    T: ?Sized + Sync,
    C: SpinContext,
{
}

impl<'a, T, C, R> SpinGuard<'a, T, C, R>
where
    T: ?Sized,
    C: SpinContext,
{
    pub(super) fn new(lock: &'a Spin<T, R>, value: &'a mut T, context: C) -> Self {
        Self {
            lock,
            value,
            context: Some(context),
            _not_send: PhantomData,
        }
    }
}

impl<T, C, R> Drop for SpinGuard<'_, T, C, R>
where
    T: ?Sized,
    C: SpinContext,
{
    fn drop(&mut self) {
        unsafe {
            // SAFETY: We are dropping the guard, so we are not holding the lock anymore.
            self.lock.do_unlock();

            self.context
                .take()
                .expect("We should have a context here")
                .restore();
        }
    }
}

impl<T, C, R> Deref for SpinGuard<'_, T, C, R>
where
    T: ?Sized,
    C: SpinContext,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: We are holding the lock, so we can safely access the value.
        self.value
    }
}

impl<T, C, R> DerefMut for SpinGuard<'_, T, C, R>
where
    T: ?Sized,
    C: SpinContext,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: We are holding the lock, so we can safely access the value.
        self.value
    }
}

impl<T, U, C, R> AsRef<U> for SpinGuard<'_, T, C, R>
where
    T: ?Sized,
    C: SpinContext,
    U: ?Sized,
    <Self as Deref>::Target: AsRef<U>,
{
    fn as_ref(&self) -> &U {
        self.deref().as_ref()
    }
}

impl<T, U, C, R> AsMut<U> for SpinGuard<'_, T, C, R>
where
    T: ?Sized,
    C: SpinContext,
    U: ?Sized,
    <Self as Deref>::Target: AsMut<U>,
{
    fn as_mut(&mut self) -> &mut U {
        self.deref_mut().as_mut()
    }
}

impl<'a, T, C, R> UnlockableGuard for SpinGuard<'a, T, C, R>
where
    T: ?Sized + Send,
    C: ContextUnlock,
    C::Unlocked: Send,
    R: Relax,
{
    type Unlocked = UnlockedSpinGuard<'a, T, C, R>;

    fn unlock(self) -> Self::Unlocked {
        let mut me = ManuallyDrop::new(self);
        unsafe {
            // SAFETY: No access is possible after unlocking.
            me.lock.do_unlock();
        }

        let unlocked_context = me
            .context
            .take()
            .expect("We should have a context here")
            .unlock();

        UnlockedSpinGuard(me.lock, unlocked_context)
    }
}

// SAFETY: The guard is stateless so no more process needed.
unsafe impl<'a, T, C, R> UnlockedGuard for UnlockedSpinGuard<'a, T, C, R>
where
    T: ?Sized + Send,
    C: ContextUnlock,
    C::Unlocked: Send,
    R: Relax,
{
    type Guard = SpinGuard<'a, T, C, R>;

    async fn relock(self) -> Self::Guard {
        let Self(lock, context) = self;

        let context = context.relock();
        lock.do_lock();

        SpinGuard::new(lock, unsafe { &mut *lock.value.get() }, context)
    }
}
