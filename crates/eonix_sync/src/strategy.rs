pub unsafe trait LockStrategy {
    type StrategyData;
    type GuardContext;

    fn new_data() -> Self::StrategyData
    where
        Self: Sized;

    unsafe fn is_locked(data: &Self::StrategyData) -> bool
    where
        Self: Sized;

    unsafe fn try_lock(data: &Self::StrategyData) -> Option<Self::GuardContext>
    where
        Self: Sized;

    unsafe fn do_lock(data: &Self::StrategyData) -> Self::GuardContext
    where
        Self: Sized;

    unsafe fn do_unlock(data: &Self::StrategyData, context: &mut Self::GuardContext)
    where
        Self: Sized;

    unsafe fn try_lock_shared(data: &Self::StrategyData) -> Option<Self::GuardContext>
    where
        Self: Sized,
    {
        unsafe { Self::try_lock(data) }
    }

    unsafe fn do_lock_shared(data: &Self::StrategyData) -> Self::GuardContext
    where
        Self: Sized,
    {
        unsafe { Self::do_lock(data) }
    }

    unsafe fn do_unlock_shared(data: &Self::StrategyData, context: &mut Self::GuardContext)
    where
        Self: Sized,
    {
        unsafe { Self::do_unlock(data, context) }
    }

    unsafe fn do_temporary_unlock(data: &Self::StrategyData, context: &mut Self::GuardContext)
    where
        Self: Sized,
    {
        unsafe { Self::do_unlock(data, context) }
    }

    unsafe fn do_temporary_unlock_shared(
        data: &Self::StrategyData,
        context: &mut Self::GuardContext,
    ) where
        Self: Sized,
    {
        unsafe { Self::do_unlock_shared(data, context) }
    }

    unsafe fn do_relock(data: &Self::StrategyData, context: &mut Self::GuardContext)
    where
        Self: Sized,
    {
        *context = unsafe { Self::do_lock(data) };
    }

    unsafe fn do_relock_shared(data: &Self::StrategyData, context: &mut Self::GuardContext)
    where
        Self: Sized,
    {
        *context = unsafe { Self::do_lock_shared(data) };
    }
}
