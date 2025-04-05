mod arcswap;
mod condvar;
mod locked;
pub mod semaphore;

pub use eonix_sync::{Guard, Lock, Spin, SpinStrategy};

#[no_mangle]
pub extern "C" fn r_preempt_disable() {
    eonix_preempt::disable();
}

#[no_mangle]
pub extern "C" fn r_preempt_enable() {
    eonix_preempt::enable();
}

#[no_mangle]
pub extern "C" fn r_preempt_count() -> usize {
    eonix_preempt::count()
}

pub type Mutex<T> = Lock<T, semaphore::SemaphoreStrategy<1>>;
#[allow(dead_code)]
pub type Semaphore<T> = Lock<T, semaphore::SemaphoreStrategy>;
pub type RwSemaphore<T> = Lock<T, semaphore::RwSemaphoreStrategy>;

#[allow(dead_code)]
pub type SpinGuard<'lock, T> = Guard<'lock, T, SpinStrategy, SpinStrategy, true>;

#[allow(dead_code)]
pub type MutexGuard<'lock, T> =
    Guard<'lock, T, semaphore::SemaphoreStrategy<1>, semaphore::SemaphoreStrategy<1>, true>;

#[allow(dead_code)]
pub type SemGuard<'lock, T> =
    Guard<'lock, T, semaphore::SemaphoreStrategy, semaphore::SemaphoreStrategy, true>;

#[allow(dead_code)]
pub type RwSemReadGuard<'lock, T> =
    Guard<'lock, T, semaphore::RwSemaphoreStrategy, semaphore::RwSemaphoreStrategy, false>;

#[allow(dead_code)]
pub type RwSemWriteGuard<'lock, T> =
    Guard<'lock, T, semaphore::RwSemaphoreStrategy, semaphore::RwSemaphoreStrategy, true>;

pub type CondVar = condvar::CondVar<true>;
pub type UCondVar = condvar::CondVar<false>;

pub use arcswap::ArcSwap;
pub use locked::{AsRefMutPosition, AsRefPosition, Locked, RefMutPosition, RefPosition};
