mod arcswap;
mod condvar;
pub mod semaphore;

use eonix_sync::WaitStrategy;

pub use eonix_sync::{Guard, Lock, Spin, SpinStrategy};

#[doc(hidden)]
pub struct Wait {
    lock: Spin<()>,
    cv_read: UCondVar,
    cv_write: UCondVar,
}

impl WaitStrategy for Wait {
    type Data = Self;

    fn new_data() -> Self::Data {
        Self {
            lock: Spin::new(()),
            cv_read: UCondVar::new(),
            cv_write: UCondVar::new(),
        }
    }

    fn has_write_waiting(data: &Self::Data) -> bool {
        data.cv_write.has_waiters()
    }

    fn has_read_waiting(data: &Self::Data) -> bool {
        data.cv_read.has_waiters()
    }

    fn write_wait(data: &Self::Data, check: impl Fn() -> bool) {
        let mut lock = data.lock.lock();

        loop {
            if check() {
                break;
            }
            data.cv_write.wait(&mut lock);
        }
    }

    fn read_wait(data: &Self::Data, check: impl Fn() -> bool) {
        let mut lock = data.lock.lock();
        loop {
            if check() {
                break;
            }
            data.cv_read.wait(&mut lock);
        }
    }

    fn write_notify(data: &Self::Data) {
        let _lock = data.lock.lock();
        if Self::has_write_waiting(data) {
            data.cv_write.notify_one();
        }
        if Self::has_read_waiting(data) {
            data.cv_read.notify_all();
        }
    }

    fn read_notify(data: &Self::Data) {
        let _lock = data.lock.lock();
        if Self::has_write_waiting(data) {
            data.cv_write.notify_one();
        }
        if Self::has_read_waiting(data) {
            data.cv_read.notify_all();
        }
    }
}

pub type Mutex<T> = Lock<T, semaphore::SemaphoreStrategy<1>>;
pub type RwLock<T> = eonix_sync::RwLock<T, Wait>;

pub type SpinGuard<'lock, T> = Guard<'lock, T, SpinStrategy, SpinStrategy, true>;
pub type RwLockReadGuard<'lock, T> =
    Guard<'lock, T, eonix_sync::RwLockStrategy<Wait>, eonix_sync::RwLockStrategy<Wait>, false>;

pub type CondVar = condvar::CondVar<true>;
pub type UCondVar = condvar::CondVar<false>;

pub use arcswap::ArcSwap;
