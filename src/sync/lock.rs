use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

use super::{spin::IrqStrategy, strategy::LockStrategy};

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

impl<Value: ?Sized, Strategy: LockStrategy> Lock<Value, Strategy> {
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

pub struct Guard<'lock, Value: ?Sized, Strategy: LockStrategy, const Write: bool = true> {
    _phantom: core::marker::PhantomData<Strategy>,
    value: &'lock UnsafeCell<Value>,
    strategy_data: &'lock Strategy::StrategyData,
    context: Strategy::GuardContext,
}

impl<'lock, Value: ?Sized, Strategy: LockStrategy> Guard<'lock, Value, Strategy> {
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

impl<'lock, Value: ?Sized, Strategy: LockStrategy, const Write: bool> Deref
    for Guard<'lock, Value, Strategy, Write>
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

impl<'lock, Value: ?Sized, Strategy: LockStrategy, const Write: bool> AsRef<Value>
    for Guard<'lock, Value, Strategy, Write>
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

impl<'lock, Value: ?Sized, Strategy: LockStrategy, const Write: bool> Drop
    for Guard<'lock, Value, Strategy, Write>
{
    fn drop(&mut self) {
        unsafe { Strategy::do_unlock(&self.strategy_data, &mut self.context) }
    }
}
