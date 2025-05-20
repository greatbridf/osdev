use super::{Relax, SpinGuard, SpinRelax, UnlockedSpinGuard};
use crate::{marker::NotSend, UnlockableGuard, UnlockedGuard};
use core::{
    marker::PhantomData,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
};

pub(super) struct IrqStateGuard(ManuallyDrop<arch::IrqState>);

pub struct SpinIrqGuard<'a, T, R = SpinRelax>
where
    T: ?Sized,
{
    pub(super) guard: SpinGuard<'a, T, R>,
    pub(super) irq_state: IrqStateGuard,
    /// We don't want this to be `Send` because we don't want to allow the guard to be
    /// transferred to another thread since we have disabled the preemption and saved
    /// IRQ states on the local cpu.
    pub(super) _not_send: PhantomData<NotSend>,
}

pub struct UnlockedSpinIrqGuard<'a, T, R>
where
    T: ?Sized,
{
    unlocked_guard: UnlockedSpinGuard<'a, T, R>,
    irq_state: IrqStateGuard,
}

// SAFETY: As long as the value protected by the lock is able to be shared between threads,
//         we can access the guard from multiple threads.
unsafe impl<T, R> Sync for SpinIrqGuard<'_, T, R> where T: ?Sized + Sync {}

impl IrqStateGuard {
    pub const fn new(irq_state: arch::IrqState) -> Self {
        Self(ManuallyDrop::new(irq_state))
    }
}

impl Drop for IrqStateGuard {
    fn drop(&mut self) {
        let Self(irq_state) = self;

        unsafe {
            // SAFETY: We are dropping the guard, so we are never going to access the value.
            ManuallyDrop::take(irq_state).restore();
        }
    }
}

impl<T, R> Deref for SpinIrqGuard<'_, T, R>
where
    T: ?Sized,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.deref()
    }
}

impl<T, R> DerefMut for SpinIrqGuard<'_, T, R>
where
    T: ?Sized,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.deref_mut()
    }
}

impl<T, U, R> AsRef<U> for SpinIrqGuard<'_, T, R>
where
    T: ?Sized,
    U: ?Sized,
    <Self as Deref>::Target: AsRef<U>,
{
    fn as_ref(&self) -> &U {
        self.deref().as_ref()
    }
}

impl<T, U, R> AsMut<U> for SpinIrqGuard<'_, T, R>
where
    T: ?Sized,
    U: ?Sized,
    <Self as Deref>::Target: AsMut<U>,
{
    fn as_mut(&mut self) -> &mut U {
        self.deref_mut().as_mut()
    }
}

impl<'a, T, R> UnlockableGuard for SpinIrqGuard<'a, T, R>
where
    T: ?Sized + Send,
    R: Relax,
{
    type Unlocked = UnlockedSpinIrqGuard<'a, T, R>;

    fn unlock(self) -> Self::Unlocked {
        UnlockedSpinIrqGuard {
            unlocked_guard: self.guard.unlock(),
            irq_state: self.irq_state,
        }
    }
}

// SAFETY: The guard is stateless so no more process needed.
unsafe impl<'a, T, R> UnlockedGuard for UnlockedSpinIrqGuard<'a, T, R>
where
    T: ?Sized + Send,
    R: Relax,
{
    type Guard = SpinIrqGuard<'a, T, R>;

    async fn relock(self) -> Self::Guard {
        SpinIrqGuard {
            guard: self.unlocked_guard.relock().await,
            irq_state: self.irq_state,
            _not_send: PhantomData,
        }
    }
}
