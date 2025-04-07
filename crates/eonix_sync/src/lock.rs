use super::strategy::LockStrategy;
use crate::Guard;
use core::{arch::asm, cell::UnsafeCell, fmt, marker::PhantomData};

pub struct IrqStrategy<Strategy: LockStrategy>(PhantomData<Strategy>);

pub struct Lock<T, S>
where
    T: ?Sized,
    S: LockStrategy,
{
    pub(crate) strategy_data: S::StrategyData,
    pub(crate) value: UnsafeCell<T>,
}

// SAFETY: As long as the value protected by the lock is able to be shared between threads,
//         the lock itself is also able to be shared between threads.
unsafe impl<T, S> Send for Lock<T, S>
where
    T: ?Sized + Send,
    S: LockStrategy,
{
}

// SAFETY: As long as the value protected by the lock is able to be shared between threads,
//         the lock will provide synchronization between threads.
unsafe impl<T, S> Sync for Lock<T, S>
where
    T: ?Sized + Send,
    S: LockStrategy,
{
}

impl<T, S> Lock<T, S>
where
    S: LockStrategy,
{
    pub fn new(value: T) -> Self {
        Self {
            strategy_data: S::new_data(),
            value: UnsafeCell::new(value),
        }
    }
}

impl<T, S> Lock<T, S>
where
    T: ?Sized,
    S: LockStrategy,
{
    pub fn is_locked(&self) -> bool {
        unsafe { S::is_locked(&self.strategy_data) }
    }

    pub fn try_lock(&self) -> Option<Guard<T, S, S>> {
        if !unsafe { S::is_locked(&self.strategy_data) } {
            unsafe { S::try_lock(&self.strategy_data) }.map(|context| Guard {
                lock: self,
                strategy_data: &self.strategy_data,
                context,
            })
        } else {
            None
        }
    }

    pub fn lock(&self) -> Guard<T, S, S> {
        Guard {
            lock: self,
            strategy_data: &self.strategy_data,
            context: unsafe { S::do_lock(&self.strategy_data) },
        }
    }

    pub fn lock_irq(&self) -> Guard<T, IrqStrategy<S>, S> {
        Guard {
            lock: self,
            strategy_data: &self.strategy_data,
            context: unsafe { IrqStrategy::<S>::do_lock(&self.strategy_data) },
        }
    }

    pub fn lock_shared(&self) -> Guard<T, S, S, false> {
        Guard {
            lock: self,
            strategy_data: &self.strategy_data,
            context: unsafe { S::do_lock_shared(&self.strategy_data) },
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.value.get() }
    }
}

impl<T, S> fmt::Debug for Lock<T, S>
where
    T: fmt::Debug,
    S: LockStrategy,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lock")
            .field("locked_value", &self.value)
            .finish()
    }
}

impl<T, S> Clone for Lock<T, S>
where
    T: Clone,
    S: LockStrategy,
{
    fn clone(&self) -> Self {
        Self {
            strategy_data: S::new_data(),
            value: UnsafeCell::new(self.lock_shared().clone()),
        }
    }
}

impl<T, S> Default for Lock<T, S>
where
    T: Default,
    S: LockStrategy,
{
    fn default() -> Self {
        Self {
            strategy_data: S::new_data(),
            value: Default::default(),
        }
    }
}

unsafe impl<Strategy: LockStrategy> LockStrategy for IrqStrategy<Strategy> {
    type StrategyData = Strategy::StrategyData;
    type GuardContext = (Strategy::GuardContext, usize);

    fn new_data() -> Self::StrategyData {
        Strategy::new_data()
    }

    unsafe fn do_lock(data: &Self::StrategyData) -> Self::GuardContext {
        let mut context: usize;

        unsafe {
            asm!(
                "pushf",
                "pop {context}",
                "cli",
                context = out(reg) context,
            );
        }

        unsafe { (Strategy::do_lock(data), context) }
    }

    unsafe fn do_unlock(data: &Self::StrategyData, context: &mut Self::GuardContext) {
        unsafe {
            Strategy::do_unlock(data, &mut context.0);

            asm!(
                "push {context}",
                "popf",
                context = in(reg) context.1,
                options(nomem),
            )
        }
    }

    unsafe fn do_temporary_unlock(data: &Self::StrategyData, context: &mut Self::GuardContext) {
        unsafe { Strategy::do_unlock(data, &mut context.0) }
    }

    unsafe fn do_relock(data: &Self::StrategyData, context: &mut Self::GuardContext) {
        unsafe { Strategy::do_relock(data, &mut context.0) }
    }

    unsafe fn is_locked(data: &Self::StrategyData) -> bool {
        unsafe { Strategy::is_locked(data) }
    }

    unsafe fn try_lock(data: &Self::StrategyData) -> Option<Self::GuardContext> {
        let mut irq_context: usize;
        unsafe {
            asm!(
                "pushf",
                "pop {context}",
                "cli",
                context = out(reg) irq_context,
            );
        }

        let lock_context = unsafe { Strategy::try_lock(data) };
        lock_context.map(|lock_context| (lock_context, irq_context))
    }
}
