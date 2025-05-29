#![no_std]

use core::sync::atomic::{compiler_fence, Ordering};

#[eonix_percpu::define_percpu]
static PREEMPT_COUNT: usize = 0;

#[inline(always)]
pub fn disable() {
    PREEMPT_COUNT.add(1);
    compiler_fence(Ordering::AcqRel);
}

#[inline(always)]
pub fn enable() {
    compiler_fence(Ordering::AcqRel);
    PREEMPT_COUNT.sub(1);
}

#[inline(always)]
pub fn count() -> usize {
    PREEMPT_COUNT.get()
}

#[macro_export]
macro_rules! assert_preempt_enabled {
    () => {{
        assert_eq!($crate::count(), 0, "Preemption is not enabled",);
    }};

    ($msg:literal) => {{
        assert_eq!($crate::count(), 0, "{}: Preemption is not enabled", $msg,);
    }};
}

#[macro_export]
macro_rules! assert_preempt_disabled {
    () => {{
        assert_ne!($crate::count(), 0, "Preemption is not disabled",);
    }};

    ($msg:literal) => {{
        assert_ne!($crate::count(), 0, "{}: Preemption is not disabled", $msg,);
    }};
}

#[macro_export]
macro_rules! assert_preempt_count_eq {
    ($n:expr) => {{
        assert_eq!(
            $crate::count(),
            $n,
            "Preemption count does not equal to {}",
            $n,
        );
    }};

    ($n:expr, $msg:literal) => {{
        assert_eq!(
            $crate::count(),
            $n,
            "{}: Preemption count does not equal to {}",
            $msg,
            $n,
        );
    }};
}

#[unsafe(no_mangle)]
pub extern "C" fn r_preempt_disable() {
    disable();
}

#[unsafe(no_mangle)]
pub extern "C" fn r_preempt_enable() {
    enable();
}

#[unsafe(no_mangle)]
pub extern "C" fn r_preempt_count() -> usize {
    count()
}
