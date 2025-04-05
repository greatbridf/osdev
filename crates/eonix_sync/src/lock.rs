use super::{spin::IrqStrategy, strategy::LockStrategy};
use crate::Guard;
use core::{cell::UnsafeCell, fmt};

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
    #[inline(always)]
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

    pub fn lock_shared_irq(&self) -> Guard<T, IrqStrategy<S>, S, false> {
        Guard {
            lock: self,
            strategy_data: &self.strategy_data,
            context: unsafe { IrqStrategy::<S>::do_lock(&self.strategy_data) },
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
