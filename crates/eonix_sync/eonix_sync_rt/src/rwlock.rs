mod guard;

use crate::WaitList;
use core::{
    cell::UnsafeCell,
    pin::pin,
    sync::atomic::{AtomicIsize, Ordering},
};

pub use guard::{RwLockReadGuard, RwLockWriteGuard};

#[derive(Debug, Default)]
pub struct RwLock<T>
where
    T: ?Sized,
{
    counter: AtomicIsize,
    read_wait: WaitList,
    write_wait: WaitList,
    value: UnsafeCell<T>,
}

impl<T> RwLock<T> {
    pub const fn new(value: T) -> Self {
        Self {
            counter: AtomicIsize::new(0),
            read_wait: WaitList::new(),
            write_wait: WaitList::new(),
            value: UnsafeCell::new(value),
        }
    }
}

impl<T> RwLock<T>
where
    T: ?Sized,
{
    /// # Safety
    /// This function is unsafe because the caller MUST ensure that we've got the
    /// write access before calling this function.
    unsafe fn write_lock(&self) -> RwLockWriteGuard<'_, T> {
        RwLockWriteGuard {
            lock: self,
            // SAFETY: We are holding the write lock, so we can safely access the value.
            value: unsafe { &mut *self.value.get() },
        }
    }

    /// # Safety
    /// This function is unsafe because the caller MUST ensure that we've got the
    /// read access before calling this function.
    unsafe fn read_lock(&self) -> RwLockReadGuard<'_, T> {
        RwLockReadGuard {
            lock: self,
            // SAFETY: We are holding the read lock, so we can safely access the value.
            value: unsafe { &*self.value.get() },
        }
    }

    /// # Safety
    /// This function is unsafe because the caller MUST ensure that we won't hold any
    /// references to the value after calling this function.
    pub(self) unsafe fn write_unlock(&self) {
        let old = self.counter.swap(0, Ordering::Release);
        debug_assert_eq!(
            old, -1,
            "RwLock::write_unlock(): erroneous counter value: {}",
            old
        );
        if !self.write_wait.notify_one() {
            self.read_wait.notify_all();
        }
    }

    /// # Safety
    /// This function is unsafe because the caller MUST ensure that we won't hold any
    /// references to the value after calling this function.
    pub(self) unsafe fn read_unlock(&self) {
        match self.counter.fetch_sub(1, Ordering::Release) {
            2.. => {}
            1 => {
                if !self.write_wait.notify_one() {
                    self.read_wait.notify_all();
                }
            }
            val => unreachable!("RwLock::read_unlock(): erroneous counter value: {}", val),
        }
    }

    pub fn try_write(&self) -> Option<RwLockWriteGuard<'_, T>> {
        self.counter
            .compare_exchange(0, -1, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| unsafe { self.write_lock() })
    }

    fn try_write_weak(&self) -> Option<RwLockWriteGuard<'_, T>> {
        self.counter
            .compare_exchange_weak(0, -1, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| unsafe { self.write_lock() })
    }

    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, T>> {
        // We'll spin if we fail here anyway.
        if self.write_wait.has_waiters() {
            return None;
        }

        let counter = self.counter.load(Ordering::Relaxed);
        if counter >= 0 {
            self.counter
                .compare_exchange(counter, counter + 1, Ordering::Acquire, Ordering::Relaxed)
                .ok()
                .map(|_| unsafe { self.read_lock() })
        } else {
            None
        }
    }

    fn try_read_weak(&self) -> Option<RwLockReadGuard<'_, T>> {
        // TODO: If we check write waiters here, we would lose wakeups.
        //       Try locking the wait lists to prevent this.

        let counter = self.counter.load(Ordering::Relaxed);
        if counter >= 0 {
            self.counter
                .compare_exchange_weak(counter, counter + 1, Ordering::Acquire, Ordering::Relaxed)
                .ok()
                .map(|_| unsafe { self.read_lock() })
        } else {
            None
        }
    }

    #[cold]
    async fn write_slow_path(&self) -> RwLockWriteGuard<'_, T> {
        loop {
            let mut wait = pin!(self.write_wait.prepare_to_wait());
            wait.as_mut().add_to_wait_list();

            if let Some(guard) = self.try_write_weak() {
                return guard;
            }

            wait.await;
        }
    }

    #[cold]
    async fn read_slow_path(&self) -> RwLockReadGuard<'_, T> {
        loop {
            let mut wait = pin!(self.read_wait.prepare_to_wait());
            wait.as_mut().add_to_wait_list();

            if let Some(guard) = self.try_read_weak() {
                return guard;
            }

            wait.await;
        }
    }

    pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
        if let Some(guard) = self.try_write() {
            // Quick path
            guard
        } else {
            self.write_slow_path().await
        }
    }

    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        if let Some(guard) = self.try_read() {
            // Quick path
            guard
        } else {
            self.read_slow_path().await
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        // SAFETY: The exclusive access to the lock is guaranteed by the borrow checker.
        unsafe { &mut *self.value.get() }
    }
}

// SAFETY: As long as the value protected by the lock is able to be shared between threads,
//         we can send the lock between threads.
unsafe impl<T> Send for RwLock<T> where T: ?Sized + Send {}

// SAFETY: `RwLock` can provide shared access to the value it protects, so it is safe to
//         implement `Sync` for it. However, this is only true if the value itself is `Sync`.
unsafe impl<T> Sync for RwLock<T> where T: ?Sized + Send + Sync {}
