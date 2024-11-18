use core::{
    arch::asm,
    sync::atomic::{AtomicBool, Ordering},
};

use super::{preempt, strategy::LockStrategy};

pub struct SpinStrategy;

impl SpinStrategy {
    #[inline(always)]
    fn is_locked(data: &<Self as LockStrategy>::StrategyData) -> bool {
        data.load(Ordering::Relaxed)
    }
}

unsafe impl LockStrategy for SpinStrategy {
    type StrategyData = AtomicBool;
    type GuardContext = ();

    #[inline(always)]
    fn data() -> Self::StrategyData {
        AtomicBool::new(false)
    }

    #[inline(always)]
    unsafe fn do_lock(data: &Self::StrategyData) -> Self::GuardContext {
        use Ordering::{Acquire, Relaxed};
        preempt::disable();

        while data
            .compare_exchange_weak(false, true, Acquire, Relaxed)
            .is_err()
        {
            while Self::is_locked(data) {
                core::hint::spin_loop();
            }
        }
    }

    #[inline(always)]
    unsafe fn do_unlock(data: &Self::StrategyData, _: &mut Self::GuardContext) {
        data.store(false, Ordering::Release);
        preempt::enable();
    }
}

pub struct IrqStrategy<Strategy: LockStrategy> {
    _phantom: core::marker::PhantomData<Strategy>,
}

unsafe impl<Strategy: LockStrategy> LockStrategy for IrqStrategy<Strategy> {
    type StrategyData = Strategy::StrategyData;
    type GuardContext = (Strategy::GuardContext, usize);

    #[inline(always)]
    fn data() -> Self::StrategyData {
        Strategy::data()
    }

    #[inline(always)]
    unsafe fn do_lock(data: &Self::StrategyData) -> Self::GuardContext {
        let mut context: usize;
        asm!(
            "pushf",
            "pop {context}",
            "cli",
            context = out(reg) context,
        );

        (Strategy::do_lock(data), context)
    }

    #[inline(always)]
    unsafe fn do_unlock(data: &Self::StrategyData, context: &mut Self::GuardContext) {
        Strategy::do_unlock(data, &mut context.0);

        asm!(
            "push {context}",
            "popf",
            context = in(reg) context.1,
            options(nomem),
        )
    }

    #[inline(always)]
    unsafe fn do_temporary_unlock(data: &Self::StrategyData, context: &mut Self::GuardContext) {
        Strategy::do_unlock(data, &mut context.0)
    }

    #[inline(always)]
    unsafe fn do_relock(data: &Self::StrategyData, context: &mut Self::GuardContext) {
        Strategy::do_relock(data, &mut context.0);
    }
}
