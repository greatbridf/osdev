pub mod condvar;
pub mod lock;
pub mod semaphore;
pub mod spin;
pub mod strategy;

extern "C" {
    fn r_preempt_disable();
    fn r_preempt_enable();
}

#[inline(always)]
fn preempt_disable() {
    unsafe {
        r_preempt_disable();
    }
}

#[inline(always)]
fn preempt_enable() {
    unsafe {
        r_preempt_enable();
    }
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

pub struct Locked<T: Sized, U: ?Sized> {
    inner: T,
    guard: *const U,
}

unsafe impl<T: Sized + Send, U: ?Sized> Send for Locked<T, U> {}
unsafe impl<T: Sized + Send + Sync, U: ?Sized> Sync for Locked<T, U> {}

impl<T: Sized + Sync, U: ?Sized> Locked<T, U> {
    pub fn new(value: T, from: &U) -> Self {
        Self {
            inner: value,
            guard: from,
        }
    }

    pub fn access<'lt>(&'lt self, guard: &'lt U) -> &'lt T {
        assert_eq!(self.guard, guard as *const U, "wrong guard");
        &self.inner
    }

    pub fn access_mut<'lt>(&'lt self, guard: &'lt mut U) -> &'lt mut T {
        assert_eq!(self.guard, guard as *const U, "wrong guard");
        unsafe { &mut *(&raw const self.inner as *mut T) }
    }
}

macro_rules! might_sleep {
    () => {
        if cfg!(debug_assertions) {
            if unsafe { $crate::bindings::root::kernel::async_::preempt_count() } != 0 {
                println_fatal!("failed assertion");
                unsafe { $crate::bindings::root::freeze() };
            }
        } else {
            assert_eq!(
                unsafe { $crate::bindings::root::kernel::async_::preempt_count() },
                0,
                "a might_sleep function called with preempt disabled"
            );
        }
    };
    ($n:expr) => {
        if cfg!(debug_assertions) {
            if unsafe { $crate::bindings::root::kernel::async_::preempt_count() } != $n {
                println_fatal!("failed assertion");
                unsafe { $crate::bindings::root::freeze() };
            }
        } else {
            assert_eq!(
                unsafe { $crate::bindings::root::kernel::async_::preempt_count() },
                $n,
                "a might_sleep function called with the preempt count not satisfying its requirement",
            );
        }
    };
}

pub(crate) use might_sleep;
