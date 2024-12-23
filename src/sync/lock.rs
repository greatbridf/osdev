use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

use super::{
    semaphore::{RwSemaphoreStrategy, SemaphoreStrategy},
    spin::IrqStrategy,
    strategy::LockStrategy,
    RwSemWriteGuard, SemGuard,
};

pub struct Lock<Value: ?Sized, Strategy: LockStrategy> {
    strategy_data: Strategy::StrategyData,
    value: UnsafeCell<Value>,
}

unsafe impl<T: ?Sized + Send, S: LockStrategy> Send for Lock<T, S> {}
unsafe impl<T: ?Sized + Send, S: LockStrategy> Sync for Lock<T, S> {}

impl<Value, Strategy: LockStrategy> Lock<Value, Strategy> {
    #[inline(always)]
    pub fn new(value: Value) -> Self {
        Self {
            strategy_data: Strategy::data(),
            value: UnsafeCell::new(value),
        }
    }
}

impl<Value: core::fmt::Debug, Strategy: LockStrategy> core::fmt::Debug for Lock<Value, Strategy> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Lock")
            .field("locked_value", &self.value)
            .finish()
    }
}

impl<Value: Clone, Strategy: LockStrategy> Clone for Lock<Value, Strategy> {
    fn clone(&self) -> Self {
        Self {
            strategy_data: Strategy::data(),
            value: UnsafeCell::new(self.lock_shared().clone()),
        }
    }
}

impl<Value: Default, Strategy: LockStrategy> Default for Lock<Value, Strategy> {
    fn default() -> Self {
        Self {
            strategy_data: Strategy::data(),
            value: Default::default(),
        }
    }
}

#[allow(dead_code)]
impl<Value: ?Sized> Lock<Value, SemaphoreStrategy> {
    #[inline(always)]
    pub fn lock_nosleep(&self) -> SemGuard<'_, Value> {
        loop {
            if !self.is_locked() {
                if let Some(guard) = self.try_lock() {
                    return guard;
                }
            }

            arch::pause();
        }
    }
}

impl<Value: ?Sized> Lock<Value, RwSemaphoreStrategy> {
    #[inline(always)]
    pub fn lock_nosleep(&self) -> RwSemWriteGuard<'_, Value> {
        loop {
            if self.is_locked() {
                if let Some(guard) = self.try_lock() {
                    return guard;
                }
            }

            arch::pause();
        }
    }
}

#[allow(dead_code)]
impl<Value: ?Sized, Strategy: LockStrategy> Lock<Value, Strategy> {
    #[inline(always)]
    pub fn is_locked(&self) -> bool {
        unsafe { Strategy::is_locked(&self.strategy_data) }
    }

    #[inline(always)]
    pub fn try_lock<'lt>(&'lt self) -> Option<Guard<'lt, Value, Strategy>> {
        if unsafe { Strategy::is_locked(&self.strategy_data) } {
            return None;
        }

        unsafe { Strategy::try_lock(&self.strategy_data) }.map(|context| Guard {
            _phantom: core::marker::PhantomData,
            value: &self.value,
            strategy_data: &self.strategy_data,
            context,
        })
    }

    #[inline(always)]
    pub fn lock<'lt>(&'lt self) -> Guard<'lt, Value, Strategy> {
        Guard {
            _phantom: core::marker::PhantomData,
            value: &self.value,
            strategy_data: &self.strategy_data,
            context: unsafe { Strategy::do_lock(&self.strategy_data) },
        }
    }

    #[inline(always)]
    pub fn lock_irq<'lt>(&'lt self) -> Guard<'lt, Value, IrqStrategy<Strategy>> {
        Guard {
            _phantom: core::marker::PhantomData,
            value: &self.value,
            strategy_data: &self.strategy_data,
            context: unsafe { IrqStrategy::<Strategy>::do_lock(&self.strategy_data) },
        }
    }

    #[inline(always)]
    pub fn lock_shared<'lt>(&'lt self) -> Guard<'lt, Value, Strategy, false> {
        Guard {
            _phantom: core::marker::PhantomData,
            value: &self.value,
            strategy_data: &self.strategy_data,
            context: unsafe { Strategy::do_lock_shared(&self.strategy_data) },
        }
    }

    #[inline(always)]
    pub fn lock_shared_irq<'lt>(&'lt self) -> Guard<'lt, Value, IrqStrategy<Strategy>, false> {
        Guard {
            _phantom: core::marker::PhantomData,
            value: &self.value,
            strategy_data: &self.strategy_data,
            context: unsafe { IrqStrategy::<Strategy>::do_lock(&self.strategy_data) },
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut Value {
        unsafe { &mut *self.value.get() }
    }
}

pub struct Guard<'lock, Value: ?Sized, Strategy: LockStrategy, const WRITE: bool = true> {
    _phantom: core::marker::PhantomData<Strategy>,
    value: &'lock UnsafeCell<Value>,
    strategy_data: &'lock Strategy::StrategyData,
    context: Strategy::GuardContext,
}

impl<'lock, Value: ?Sized, Strategy: LockStrategy, const W: bool> Guard<'lock, Value, Strategy, W> {
    /// # Safety
    /// Use of the lock after calling this function without relocking is undefined behavior.
    #[inline(always)]
    pub unsafe fn force_unlock(&mut self) {
        Strategy::do_temporary_unlock(&self.strategy_data, &mut self.context)
    }

    /// # Safety
    /// Calling this function more than once will cause deadlocks.
    #[inline(always)]
    pub unsafe fn force_relock(&mut self) {
        Strategy::do_relock(&self.strategy_data, &mut self.context)
    }
}

impl<'lock, Value: ?Sized, Strategy: LockStrategy, const WRITE: bool> Deref
    for Guard<'lock, Value, Strategy, WRITE>
{
    type Target = Value;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.value.get() }
    }
}

impl<'lock, Value: ?Sized, Strategy: LockStrategy> DerefMut
    for Guard<'lock, Value, Strategy, true>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.value.get() }
    }
}

impl<'lock, Value: ?Sized, Strategy: LockStrategy, const WRITE: bool> AsRef<Value>
    for Guard<'lock, Value, Strategy, WRITE>
{
    fn as_ref(&self) -> &Value {
        unsafe { &*self.value.get() }
    }
}

impl<'lock, Value: ?Sized, Strategy: LockStrategy> AsMut<Value>
    for Guard<'lock, Value, Strategy, true>
{
    fn as_mut(&mut self) -> &mut Value {
        unsafe { &mut *self.value.get() }
    }
}

impl<'lock, Value: ?Sized, Strategy: LockStrategy, const WRITE: bool> Drop
    for Guard<'lock, Value, Strategy, WRITE>
{
    fn drop(&mut self) {
        unsafe { Strategy::do_unlock(&self.strategy_data, &mut self.context) }
    }
}
