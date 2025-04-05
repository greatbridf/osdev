mod arcswap;
mod condvar;
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
pub type RwSemaphore<T> = Lock<T, semaphore::RwSemaphoreStrategy>;

pub type SpinGuard<'lock, T> = Guard<'lock, T, SpinStrategy, SpinStrategy, true>;
pub type RwSemReadGuard<'lock, T> =
    Guard<'lock, T, semaphore::RwSemaphoreStrategy, semaphore::RwSemaphoreStrategy, false>;

pub type CondVar = condvar::CondVar<true>;
pub type UCondVar = condvar::CondVar<false>;

pub use arcswap::ArcSwap;
