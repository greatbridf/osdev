#[allow(dead_code)]
pub type KResult<T> = Result<T, u32>;

macro_rules! dont_check {
    ($arg:expr) => {
        match $arg {
            Ok(_) => (),
            Err(_) => (),
        }
    };
}

#[allow(unused_imports)]
pub(crate) use dont_check;

#[allow(unused_imports)]
pub use crate::bindings::root as bindings;

#[allow(unused_imports)]
pub(crate) use crate::kernel::console::{print, println};

#[allow(unused_imports)]
pub(crate) use alloc::{boxed::Box, string::String, vec, vec::Vec};

#[allow(unused_imports)]
pub(crate) use core::{any::Any, fmt::Write, marker::PhantomData, str};

pub struct Yield;

extern "C" {
    fn r_preempt_disable();
    fn r_preempt_enable();
}

#[inline(always)]
pub fn preempt_disable() {
    unsafe {
        r_preempt_disable();
    }
}

#[inline(always)]
pub fn preempt_enable() {
    unsafe {
        r_preempt_enable();
    }
}

impl spin::RelaxStrategy for Yield {
    fn relax() {
        panic!("ohohoh");
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub struct PreemptGuard;

impl PreemptGuard {
    #[inline(always)]
    pub fn new() -> Self {
        preempt_disable();
        Self
    }
}

impl Drop for PreemptGuard {
    #[inline(always)]
    fn drop(&mut self) {
        preempt_enable();
    }
}

#[repr(transparent)]
pub struct MutexNoPreemptionGuard<'a, T: ?Sized> {
    data_guard: spin::mutex::MutexGuard<'a, T>,
    preempt_guard: PreemptGuard,
}

impl<'a, T: ?Sized> MutexNoPreemptionGuard<'a, T> {
    #[inline(always)]
    pub fn new(
        preempt_guard: PreemptGuard,
        data_guard: spin::mutex::MutexGuard<'a, T>,
    ) -> Self {
        Self {
            data_guard,
            preempt_guard,
        }
    }
}

impl<'a, T: ?Sized> core::ops::Deref for MutexNoPreemptionGuard<'a, T> {
    type Target = <spin::mutex::MutexGuard<'a, T> as core::ops::Deref>::Target;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &*self.data_guard
    }
}

impl<'a, T: ?Sized> core::ops::DerefMut for MutexNoPreemptionGuard<'a, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.data_guard
    }
}

#[repr(transparent)]
pub struct MutexNoPreemption<T: ?Sized> {
    lock: spin::mutex::Mutex<T, spin::Spin>,
}

impl<T> MutexNoPreemption<T> {
    #[inline(always)]
    pub const fn new(value: T) -> Self {
        Self {
            lock: spin::mutex::Mutex::new(value),
        }
    }
}

#[allow(dead_code)]
impl<T: ?Sized> MutexNoPreemption<T> {
    #[inline(always)]
    pub fn lock(&self) -> MutexNoPreemptionGuard<T> {
        let preempt_guard = PreemptGuard::new();
        let data_guard = self.lock.lock();

        MutexNoPreemptionGuard::new(preempt_guard, data_guard)
    }

    #[inline(always)]
    pub fn is_locked(&self) -> bool {
        self.lock.is_locked()
    }

    #[inline(always)]
    pub fn try_lock(&self) -> Option<MutexNoPreemptionGuard<T>> {
        let preempt_guard = PreemptGuard::new();
        let data_guard = self.lock.try_lock();

        data_guard.map(|data_guard| {
            MutexNoPreemptionGuard::new(preempt_guard, data_guard)
        })
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.lock.get_mut()
    }
}

#[allow(dead_code)]
pub type RwLock<T> = spin::rwlock::RwLock<T, Yield>;
pub type Mutex<T> = MutexNoPreemption<T>;
