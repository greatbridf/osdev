mod guard;
mod relax;
mod spin_irq;

use core::{
    cell::UnsafeCell,
    marker::PhantomData,
    sync::atomic::{AtomicBool, Ordering},
};

pub use guard::{SpinGuard, UnlockedSpinGuard};
pub use relax::{LoopRelax, Relax, SpinRelax};
pub use spin_irq::SpinIrq;

pub trait SpinContext {
    fn save() -> Self;
    fn restore(self);
}

pub trait ContextUnlock: SpinContext {
    type Unlocked: UnlockedContext<Relocked = Self>;

    fn unlock(self) -> Self::Unlocked;
}

pub trait UnlockedContext {
    type Relocked: ContextUnlock<Unlocked = Self>;

    fn relock(self) -> Self::Relocked;
}

pub struct NoContext;

pub struct DisablePreemption();

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
    }
}

impl<T, R> Spin<T, R>
where
    T: ?Sized,
    R: Relax,
{
    pub fn lock_with_context<C>(&self, context: C) -> SpinGuard<T, C, R>
    where
        C: SpinContext,
    {
        self.do_lock();

        SpinGuard::new(
            self,
            unsafe {
                // SAFETY: We are holding the lock, so we can safely access the value.
                &mut *self.value.get()
            },
            context,
        )
    }

    pub fn lock(&self) -> SpinGuard<T, DisablePreemption, R> {
        self.lock_with_context(DisablePreemption::save())
    }

    pub fn get_mut(&mut self) -> &mut T {
        // SAFETY: The exclusive access to the lock is guaranteed by the borrow checker.
        unsafe { &mut *self.value.get() }
    }

    fn do_lock(&self) {
        while let Err(_) =
            self.locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        {
            R::relax();
        }
    }
}

// SAFETY: As long as the value protected by the lock is able to be shared between threads,
//         we can send the lock between threads.
unsafe impl<T, R> Send for Spin<T, R> where T: ?Sized + Send {}

// SAFETY: As long as the value protected by the lock is able to be shared between threads,
//         we can provide exclusive access guarantees to the lock.
unsafe impl<T, R> Sync for Spin<T, R> where T: ?Sized + Send {}

impl SpinContext for NoContext {
    fn save() -> Self {
        Self
    }

    fn restore(self) {}
}

impl ContextUnlock for NoContext {
    type Unlocked = NoContext;

    fn unlock(self) -> Self::Unlocked {
        self
    }
}

impl UnlockedContext for NoContext {
    type Relocked = NoContext;

    fn relock(self) -> Self::Relocked {
        self
    }
}

impl SpinContext for DisablePreemption {
    fn save() -> Self {
        eonix_preempt::disable();
        Self()
    }

    fn restore(self) {
        eonix_preempt::enable();
    }
}

impl ContextUnlock for DisablePreemption {
    type Unlocked = DisablePreemption;

    fn unlock(self) -> Self::Unlocked {
        eonix_preempt::enable();
        self
    }
}

impl UnlockedContext for DisablePreemption {
    type Relocked = DisablePreemption;

    fn relock(self) -> Self::Relocked {
        eonix_preempt::disable();
        self
    }
}
