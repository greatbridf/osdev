pub unsafe trait LockStrategy {
    type StrategyData;
    type GuardContext;

    fn data() -> Self::StrategyData;

    unsafe fn do_lock(data: &Self::StrategyData) -> Self::GuardContext;

    unsafe fn do_unlock(
        data: &Self::StrategyData,
        context: &mut Self::GuardContext,
    );

    unsafe fn do_lock_shared(data: &Self::StrategyData) -> Self::GuardContext {
        Self::do_lock(data)
    }

    #[inline(always)]
    unsafe fn do_temporary_unlock(
        data: &Self::StrategyData,
        context: &mut Self::GuardContext,
    ) {
        Self::do_unlock(data, context);
    }

    #[inline(always)]
    unsafe fn do_relock(
        data: &Self::StrategyData,
        context: &mut Self::GuardContext,
    ) {
        *context = Self::do_lock(data);
    }
}
