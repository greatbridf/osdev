use super::strategy::LockStrategy;
use core::{
    arch::asm,
    marker::PhantomData,
    sync::atomic::{AtomicBool, Ordering},
};

pub struct SpinStrategy;
pub struct IrqStrategy<Strategy: LockStrategy>(PhantomData<Strategy>);

impl SpinStrategy {
    fn is_locked(data: &<Self as LockStrategy>::StrategyData) -> bool {
        data.load(Ordering::Relaxed)
    }
}

unsafe impl LockStrategy for SpinStrategy {
    type StrategyData = AtomicBool;
    type GuardContext = ();

    fn new_data() -> Self::StrategyData {
        AtomicBool::new(false)
    }

    unsafe fn is_locked(data: &Self::StrategyData) -> bool {
        data.load(Ordering::Relaxed)
    }

    unsafe fn try_lock(data: &Self::StrategyData) -> Option<Self::GuardContext> {
        eonix_preempt::disable();

        data.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map(|_| ())
            .inspect_err(|_| eonix_preempt::enable())
            .ok()
    }

    unsafe fn do_lock(data: &Self::StrategyData) -> Self::GuardContext {
        eonix_preempt::disable();

        while data
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            while Self::is_locked(data) {
                core::hint::spin_loop();
            }
        }
    }

    unsafe fn do_unlock(data: &Self::StrategyData, _: &mut Self::GuardContext) {
        data.store(false, Ordering::Release);
        eonix_preempt::enable();
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
