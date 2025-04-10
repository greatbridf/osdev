mod arcswap;
mod condvar;

use eonix_sync::{MutexWait, RwLockWait};

pub use eonix_sync::Spin;

#[doc(hidden)]
#[derive(Debug)]
pub struct RwLockWaitImpl {
    lock: Spin<()>,
    cv_read: UCondVar,
    cv_write: UCondVar,
}

#[doc(hidden)]
#[derive(Debug)]
pub struct MutexWaitImpl {
    lock: Spin<()>,
    cv: UCondVar,
}

impl RwLockWaitImpl {
    const fn new() -> Self {
        Self {
            lock: Spin::new(()),
            cv_read: UCondVar::new(),
            cv_write: UCondVar::new(),
        }
    }
}

impl MutexWaitImpl {
    const fn new() -> Self {
        Self {
            lock: Spin::new(()),
            cv: UCondVar::new(),
        }
    }
}

impl RwLockWait for RwLockWaitImpl {
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

impl MutexWait for MutexWaitImpl {
    fn new() -> Self {
        Self::new()
    }

    fn has_waiting(&self) -> bool {
        self.cv.has_waiters()
    }

    fn wait(&self, check: impl Fn() -> bool) {
        let mut lock = self.lock.lock();
        loop {
            if check() {
                break;
            }
            self.cv.wait(&mut lock);
        }
    }

    fn notify(&self) {
        let _lock = self.lock.lock();
        if self.has_waiting() {
            self.cv.notify_one();
        }
    }
}

pub const fn rwlock_new<T>(value: T) -> RwLock<T> {
    RwLock::new(value, RwLockWaitImpl::new())
}

pub const fn mutex_new<T>(value: T) -> Mutex<T> {
    Mutex::new(value, MutexWaitImpl::new())
}

pub type RwLock<T> = eonix_sync::RwLock<T, RwLockWaitImpl>;
pub type Mutex<T> = eonix_sync::Mutex<T, MutexWaitImpl>;

pub type RwLockReadGuard<'a, T> = eonix_sync::RwLockReadGuard<'a, T, RwLockWaitImpl>;

pub type CondVar = condvar::CondVar<true>;
pub type UCondVar = condvar::CondVar<false>;

pub use arcswap::ArcSwap;
