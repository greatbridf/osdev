mod arcswap;
mod condvar;
pub mod lock;
mod locked;
pub mod semaphore;
pub mod spin;
pub mod strategy;

pub mod preempt {
    use core::sync::atomic::{compiler_fence, Ordering};

    #[arch::define_percpu]
    static PREEMPT_COUNT: usize = 0;

    #[inline(always)]
    pub fn disable() {
        PREEMPT_COUNT.add(1);
        compiler_fence(Ordering::SeqCst);
    }

    #[inline(always)]
    pub fn enable() {
        compiler_fence(Ordering::SeqCst);
        PREEMPT_COUNT.sub(1);
    }

    #[inline(always)]
    pub fn count() -> usize {
        PREEMPT_COUNT.get()
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

pub use arcswap::ArcSwap;
pub use locked::{AsRefMutPosition, AsRefPosition, Locked, RefMutPosition, RefPosition};
pub(crate) use might_sleep;
