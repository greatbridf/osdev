mod condvar;
pub mod lock;
pub mod semaphore;
pub mod spin;
pub mod strategy;

pub mod preempt {
    use core::sync::atomic::{compiler_fence, AtomicUsize, Ordering};

    /// TODO: This should be per cpu.
    static PREEMPT_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[inline(always)]
    pub fn disable() {
        PREEMPT_COUNT.fetch_add(1, Ordering::Relaxed);
        compiler_fence(Ordering::SeqCst);
    }

    #[inline(always)]
    pub fn enable() {
        compiler_fence(Ordering::SeqCst);
        PREEMPT_COUNT.fetch_sub(1, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn count() -> usize {
        PREEMPT_COUNT.load(Ordering::Relaxed)
    }
}

#[no_mangle]
pub extern "C" fn r_preempt_disable() {
    preempt::disable();
}

#[no_mangle]
pub extern "C" fn r_preempt_enable() {
    preempt::enable();
}

#[no_mangle]
pub extern "C" fn r_preempt_count() -> usize {
    preempt::count()
}

pub type Spin<T> = lock::Lock<T, spin::SpinStrategy>;
pub type Mutex<T> = lock::Lock<T, semaphore::SemaphoreStrategy<1>>;
#[allow(dead_code)]
pub type Semaphore<T> = lock::Lock<T, semaphore::SemaphoreStrategy>;
pub type RwSemaphore<T> = lock::Lock<T, semaphore::RwSemaphoreStrategy>;

#[allow(dead_code)]
pub type SpinGuard<'lock, T> = lock::Guard<'lock, T, spin::SpinStrategy, true>;

#[allow(dead_code)]
pub type MutexGuard<'lock, T> = lock::Guard<'lock, T, semaphore::SemaphoreStrategy<1>, true>;

#[allow(dead_code)]
pub type SemGuard<'lock, T> = lock::Guard<'lock, T, semaphore::SemaphoreStrategy, true>;

#[allow(dead_code)]
pub type RwSemReadGuard<'lock, T> = lock::Guard<'lock, T, semaphore::RwSemaphoreStrategy, false>;

#[allow(dead_code)]
pub type RwSemWriteGuard<'lock, T> = lock::Guard<'lock, T, semaphore::RwSemaphoreStrategy, true>;

pub type CondVar = condvar::CondVar<true>;
pub type UCondVar = condvar::CondVar<false>;

pub struct Locked<T: Sized, U: ?Sized> {
    inner: UnsafeCell<T>,
    guard: *const U,
}

unsafe impl<T: Sized + Send, U: ?Sized> Send for Locked<T, U> {}
unsafe impl<T: Sized + Send + Sync, U: ?Sized> Sync for Locked<T, U> {}

impl<T: Sized + Sync, U: ?Sized> Locked<T, U> {
    pub fn new(value: T, from: &U) -> Self {
        Self {
            inner: UnsafeCell::new(value),
            guard: from,
        }
    }

    pub fn access<'lt>(&'lt self, guard: &'lt U) -> &'lt T {
        assert_eq!(self.guard, guard as *const U, "wrong guard");
        // SAFETY: The guard protects the shared access to the inner value.
        unsafe { self.inner.get().as_ref() }.unwrap()
    }

    pub fn access_mut<'lt>(&'lt self, guard: &'lt mut U) -> &'lt mut T {
        assert_eq!(self.guard, guard as *const U, "wrong guard");
        // SAFETY: The guard protects the exclusive access to the inner value.
        unsafe { self.inner.get().as_mut() }.unwrap()
    }
}

macro_rules! might_sleep {
    () => {
        assert_eq!(
            $crate::sync::preempt::count(),
            0,
            "a might_sleep function called with preempt disabled"
        );
    };
    ($n:expr) => {
        assert_eq!(
            $crate::sync::preempt::count(),
            $n,
            "a might_sleep function called with the preempt count not satisfying its requirement",
        );
    };
}

use core::cell::UnsafeCell;

pub(crate) use might_sleep;
