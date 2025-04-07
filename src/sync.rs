mod arcswap;
mod condvar;
pub mod semaphore;

use eonix_sync::RwLockWait;
pub use eonix_sync::{Lock, Spin};

#[doc(hidden)]
#[derive(Debug)]
pub struct Wait {
    lock: Spin<()>,
    cv_read: UCondVar,
    cv_write: UCondVar,
}

impl Wait {
    const fn new() -> Self {
        Self {
            lock: Spin::new(()),
            cv_read: UCondVar::new(),
            cv_write: UCondVar::new(),
        }
    }
}

impl RwLockWait for Wait {
    fn new() -> Self {
        Self::new()
    }

    fn has_write_waiting(&self) -> bool {
        self.cv_write.has_waiters()
    }

    fn has_read_waiting(&self) -> bool {
        self.cv_read.has_waiters()
    }

    fn write_wait(&self, check: impl Fn() -> bool) {
        let mut lock = self.lock.lock();

        loop {
            if check() {
                break;
            }
            self.cv_write.wait(&mut lock);
        }
    }

    fn read_wait(&self, check: impl Fn() -> bool) {
        let mut lock = self.lock.lock();
        loop {
            if check() {
                break;
            }
            self.cv_read.wait(&mut lock);
        }
    }

    fn write_notify(&self) {
        let _lock = self.lock.lock();
        if self.has_write_waiting() {
            self.cv_write.notify_one();
        } else if self.has_read_waiting() {
            self.cv_read.notify_all();
        }
    }

    fn read_notify(&self) {
        let _lock = self.lock.lock();
        if self.has_write_waiting() {
            self.cv_write.notify_one();
        } else if self.has_read_waiting() {
            self.cv_read.notify_all();
        }
    }
}

pub const fn rwlock_new<T>(value: T) -> RwLock<T> {
    RwLock::new(value, Wait::new())
}

pub type Mutex<T> = Lock<T, semaphore::SemaphoreStrategy<1>>;
pub type RwLock<T> = eonix_sync::RwLock<T, Wait>;

pub type RwLockReadGuard<'a, T> = eonix_sync::RwLockReadGuard<'a, T, Wait>;
#[allow(dead_code)]
pub type RwLockWriteGuard<'a, T> = eonix_sync::RwLockWriteGuard<'a, T, Wait>;

pub type CondVar = condvar::CondVar<true>;
pub type UCondVar = condvar::CondVar<false>;

pub use arcswap::ArcSwap;
