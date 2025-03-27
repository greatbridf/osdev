use core::{pin::Pin, ptr::NonNull};

use arch::CPUStatus;

use super::{
    mem::{paging::Page, phys::PhysPtr as _},
    task::init_rq_thiscpu,
};

#[arch::define_percpu]
static CPU_STATUS: Option<CPUStatus> = None;

/// # Safety
/// This function is unsafe because it needs preemption to be disabled.
pub unsafe fn current_cpu() -> Pin<&'static mut CPUStatus> {
    // SAFETY: `CPU_STATUS` is global static and initialized only once.
    unsafe { Pin::new_unchecked(CPU_STATUS.as_mut().as_mut().unwrap()) }
}

pub unsafe fn init_thiscpu() {
    let status = arch::CPUStatus::new_thiscpu(|layout| {
        // TODO: Use page size defined in `arch`.
        let page_count = (layout.size() + 0x1000 - 1) / 0x1000;
        let page = Page::early_alloc_ceil(page_count);
        let pointer = page.as_cached().as_ptr();
        core::mem::forget(page);

        NonNull::new(pointer).expect("Allocated page pfn should be non-null")
    });

    CPU_STATUS.set(Some(status));

    // SAFETY: `CPU_STATUS` is global static and initialized only once.
    current_cpu().init();
    init_rq_thiscpu();
}
