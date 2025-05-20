mod guard;

use crate::WaitList;
use core::{
    cell::UnsafeCell,
    pin::pin,
    sync::atomic::{AtomicBool, Ordering},
};

pub use guard::MutexGuard;

#[derive(Debug, Default)]
pub struct Mutex<T>
where
    T: ?Sized,
{
    locked: AtomicBool,
    wait_list: WaitList,
    value: UnsafeCell<T>,
}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            wait_list: WaitList::new(),
            value: UnsafeCell::new(value),
        }
    }
}

impl<T> Mutex<T>
where
    T: ?Sized,
{
    /// # Safety
    /// This function is unsafe because the caller MUST ensure that we've got the
    /// exclusive access before calling this function.
    unsafe fn get_lock(&self) -> MutexGuard<'_, T> {
        MutexGuard {
            lock: self,
            // SAFETY: We are holding the lock, so we can safely access the value.
            value: unsafe { &mut *self.value.get() },
        }
    }

    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| unsafe { self.get_lock() })
    }

    fn try_lock_weak(&self) -> Option<MutexGuard<'_, T>> {
        self.locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| unsafe { self.get_lock() })
    }

    #[cold]
    async fn lock_slow_path(&self) -> MutexGuard<'_, T> {
        loop {
            let mut wait = pin!(self.wait_list.prepare_to_wait());
            wait.as_mut().add_to_wait_list();

            if let Some(guard) = self.try_lock_weak() {
                return guard;
            }

            wait.await;
        }
    }

    pub async fn lock(&self) -> MutexGuard<'_, T> {
        if let Some(guard) = self.try_lock() {
            // Quick path
            guard
        } else {
            self.lock_slow_path().await
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        // SAFETY: The exclusive access to the lock is guaranteed by the borrow checker.
        unsafe { &mut *self.value.get() }
    }
}

// SAFETY: As long as the value protected by the lock is able to be shared between threads,
//         we can send the lock between threads.
unsafe impl<T> Send for Mutex<T> where T: ?Sized + Send {}

// SAFETY: `RwLock` can provide exclusive access to the value it protects, so it is safe to
//         implement `Sync` for it as long as the protected value is `Send`.
unsafe impl<T> Sync for Mutex<T> where T: ?Sized + Send {}
