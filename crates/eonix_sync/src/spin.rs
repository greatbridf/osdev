mod guard;
mod relax;
mod spin_irq;

use core::{
    cell::UnsafeCell,
    marker::PhantomData,
    sync::atomic::{AtomicBool, Ordering},
};
use spin_irq::IrqStateGuard;

pub use guard::{SpinGuard, UnlockedSpinGuard};
pub use relax::{LoopRelax, Relax, SpinRelax};
pub use spin_irq::{SpinIrqGuard, UnlockedSpinIrqGuard};

//// A spinlock is a lock that uses busy-waiting to acquire the lock.
/// It is useful for short critical sections where the overhead of a context switch
/// is too high.
#[derive(Debug, Default)]
pub struct Spin<T, R = SpinRelax>
where
    T: ?Sized,
{
    _phantom: PhantomData<R>,
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

impl<T, R> Spin<T, R>
where
    R: Relax,
{
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
            _phantom: PhantomData,
        }
    }
}

impl<T, R> Spin<T, R>
where
    T: ?Sized,
{
    /// # Safety
    /// This function is unsafe because the caller MUST ensure that the protected
    /// value is no longer accessed after calling this function.
    unsafe fn do_unlock(&self) {
        let locked = self.locked.swap(false, Ordering::Release);
        debug_assert!(locked, "Spin::unlock(): Unlocking an unlocked lock");
        eonix_preempt::enable();
    }
}

impl<T, R> Spin<T, R>
where
    T: ?Sized,
    R: Relax,
{
    pub fn lock(&self) -> SpinGuard<'_, T, R> {
        self.do_lock();

        SpinGuard {
            lock: self,
            // SAFETY: We are holding the lock, so we can safely access the value.
            value: unsafe { &mut *self.value.get() },
            _not_send: PhantomData,
        }
    }

    pub fn lock_irq(&self) -> SpinIrqGuard<'_, T, R> {
        let irq_state = arch::disable_irqs_save();
        let guard = self.lock();

        SpinIrqGuard {
            guard,
            irq_state: IrqStateGuard::new(irq_state),
            _not_send: PhantomData,
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        // SAFETY: The exclusive access to the lock is guaranteed by the borrow checker.
        unsafe { &mut *self.value.get() }
    }

    fn do_lock(&self) {
        eonix_preempt::disable();

        while let Err(_) =
            self.locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        {
            R::relax();
        }
    }
}

impl<T, R> Clone for Spin<T, R>
where
    T: ?Sized + Clone,
    R: Relax,
{
    fn clone(&self) -> Self {
        Self::new(self.lock().clone())
    }
}

// SAFETY: As long as the value protected by the lock is able to be shared between threads,
//         we can send the lock between threads.
unsafe impl<T, R> Send for Spin<T, R> where T: ?Sized + Send {}

// SAFETY: As long as the value protected by the lock is able to be shared between threads,
//         we can provide exclusive access guarantees to the lock.
unsafe impl<T, R> Sync for Spin<T, R> where T: ?Sized + Send {}
