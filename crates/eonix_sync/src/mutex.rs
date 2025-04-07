mod guard;
mod wait;

use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};

pub use guard::MutexGuard;
pub use wait::Wait;

#[derive(Debug, Default)]
pub struct Mutex<T, W>
where
    T: ?Sized,
    W: Wait,
{
    locked: AtomicBool,
    wait: W,
    value: UnsafeCell<T>,
}

impl<T, W> Mutex<T, W>
where
    W: Wait,
{
    pub const fn new(value: T, wait: W) -> Self {
        Self {
            locked: AtomicBool::new(false),
            wait,
            value: UnsafeCell::new(value),
        }
    }
}

impl<T, W> Mutex<T, W>
where
    T: ?Sized,
    W: Wait,
{
    /// # Safety
    /// This function is unsafe because the caller MUST ensure that we've got the
    /// exclusive access before calling this function.
    unsafe fn get_lock(&self) -> MutexGuard<'_, T, W> {
        MutexGuard {
            lock: self,
            // SAFETY: We are holding the lock, so we can safely access the value.
            value: unsafe { &mut *self.value.get() },
        }
    }

    pub fn try_lock(&self) -> Option<MutexGuard<'_, T, W>> {
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| unsafe { self.get_lock() })
    }

    fn try_lock_weak(&self) -> Option<MutexGuard<'_, T, W>> {
        self.locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| unsafe { self.get_lock() })
    }

    #[cold]
    fn lock_slow_path(&self) -> MutexGuard<'_, T, W> {
        loop {
            if let Some(guard) = self.try_lock_weak() {
                return guard;
            }

            self.wait.wait(|| !self.locked.load(Ordering::Relaxed));
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, T, W> {
        if let Some(guard) = self.try_lock() {
            // Quick path
            guard
        } else {
            self.lock_slow_path()
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        // SAFETY: The exclusive access to the lock is guaranteed by the borrow checker.
        unsafe { &mut *self.value.get() }
    }
}

impl<T, W> Clone for Mutex<T, W>
where
    T: ?Sized + Clone,
    W: Wait,
{
    fn clone(&self) -> Self {
        Self::new(self.lock().clone(), W::new())
    }
}

// SAFETY: As long as the value protected by the lock is able to be shared between threads,
//         we can send the lock between threads.
unsafe impl<T, W> Send for Mutex<T, W>
where
    T: ?Sized + Send,
    W: Wait,
{
}

// SAFETY: `RwLock` can provide exclusive access to the value it protects, so it is safe to
//         implement `Sync` for it as long as the protected value is `Send`.
unsafe impl<T, W> Sync for Mutex<T, W>
where
    T: ?Sized + Send,
    W: Wait,
{
}
