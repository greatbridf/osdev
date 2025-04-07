mod guard;
mod wait;

use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicIsize, Ordering},
};

pub use guard::{RwLockReadGuard, RwLockWriteGuard};
pub use wait::Wait;

#[derive(Debug, Default)]
pub struct RwLock<T, W>
where
    T: ?Sized,
    W: Wait,
{
    counter: AtomicIsize,
    wait: W,
    value: UnsafeCell<T>,
}

impl<T, W> RwLock<T, W>
where
    W: Wait,
{
    pub const fn new(value: T, wait: W) -> Self {
        Self {
            counter: AtomicIsize::new(0),
            wait,
            value: UnsafeCell::new(value),
        }
    }
}

impl<T, W> RwLock<T, W>
where
    T: ?Sized,
    W: Wait,
{
    /// # Safety
    /// This function is unsafe because the caller MUST ensure that we've got the
    /// write access before calling this function.
    unsafe fn write_lock(&self) -> RwLockWriteGuard<'_, T, W> {
        RwLockWriteGuard {
            lock: self,
            // SAFETY: We are holding the write lock, so we can safely access the value.
            value: unsafe { &mut *self.value.get() },
        }
    }

    /// # Safety
    /// This function is unsafe because the caller MUST ensure that we've got the
    /// read access before calling this function.
    unsafe fn read_lock(&self) -> RwLockReadGuard<'_, T, W> {
        RwLockReadGuard {
            lock: self,
            // SAFETY: We are holding the read lock, so we can safely access the value.
            value: unsafe { &*self.value.get() },
        }
    }

    pub fn try_write(&self) -> Option<RwLockWriteGuard<'_, T, W>> {
        self.counter
            .compare_exchange(0, -1, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| unsafe { self.write_lock() })
    }

    fn try_write_weak(&self) -> Option<RwLockWriteGuard<'_, T, W>> {
        self.counter
            .compare_exchange_weak(0, -1, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| unsafe { self.write_lock() })
    }

    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, T, W>> {
        if self.wait.has_write_waiting() {
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

    fn try_read_weak(&self) -> Option<RwLockReadGuard<'_, T, W>> {
        if self.wait.has_write_waiting() {
            return None;
        }

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
    fn write_slow_path(&self) -> RwLockWriteGuard<'_, T, W> {
        loop {
            if let Some(guard) = self.try_write_weak() {
                return guard;
            }

            self.wait
                .write_wait(|| self.counter.load(Ordering::Relaxed) == 0);
        }
    }

    #[cold]
    fn read_slow_path(&self) -> RwLockReadGuard<'_, T, W> {
        loop {
            // TODO: can we use `try_read_weak` here?
            let mut counter = self.counter.load(Ordering::Relaxed);
            while counter >= 0 {
                match self.counter.compare_exchange_weak(
                    counter,
                    counter + 1,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return unsafe { self.read_lock() },
                    Err(previous) => counter = previous,
                }
            }

            self.wait
                .read_wait(|| self.counter.load(Ordering::Relaxed) >= 0);
        }
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, T, W> {
        if let Some(guard) = self.try_write() {
            // Quick path
            guard
        } else {
            self.write_slow_path()
        }
    }

    pub fn read(&self) -> RwLockReadGuard<'_, T, W> {
        if let Some(guard) = self.try_read() {
            // Quick path
            guard
        } else {
            self.read_slow_path()
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        // SAFETY: The exclusive access to the lock is guaranteed by the borrow checker.
        unsafe { &mut *self.value.get() }
    }
}

impl<T, W> Clone for RwLock<T, W>
where
    T: ?Sized + Clone,
    W: Wait,
{
    fn clone(&self) -> Self {
        Self::new(self.read().clone(), W::new())
    }
}

// SAFETY: As long as the value protected by the lock is able to be shared between threads,
//         we can send the lock between threads.
unsafe impl<T, W> Send for RwLock<T, W>
where
    T: ?Sized + Send,
    W: Wait,
{
}

// SAFETY: `RwLock` can provide shared access to the value it protects, so it is safe to
//         implement `Sync` for it. However, this is only true if the value itself is `Sync`.
unsafe impl<T, W> Sync for RwLock<T, W>
where
    T: ?Sized + Send + Sync,
    W: Wait,
{
}
