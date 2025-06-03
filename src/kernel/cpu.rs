use super::mem::{AsMemoryBlock, GlobalPageAlloc};
use arch::{PercpuArea, CPU};
use core::{alloc::Layout, pin::Pin, ptr::NonNull};
use eonix_hal::mm::PAGE_SIZE;
use eonix_mm::paging::Page;
use eonix_sync::LazyLock;

#[eonix_percpu::define_percpu]
static CPU: LazyLock<CPU> =
    LazyLock::new(|| CPU::new(unsafe { eonix_hal::trap::TRAP_STUBS_START }));

/// # Safety
/// This function is unsafe because it needs preemption to be disabled.
pub unsafe fn local_cpu() -> Pin<&'static mut CPU> {
    // SAFETY: `CPU_STATUS` is global static and initialized only once.
    unsafe { Pin::new_unchecked(CPU.as_mut().get_mut()) }
}

pub fn percpu_allocate(layout: Layout) -> NonNull<u8> {
    // TODO: Use page size defined in `arch`.
    let page_count = layout.size().div_ceil(PAGE_SIZE);
    let page = Page::alloc_at_least_in(page_count, GlobalPageAlloc::early_alloc());
    let page_data = page.as_memblk().as_byte_ptr();
    core::mem::forget(page);

    page_data
}

pub fn init_localcpu() {
    let percpu_area = PercpuArea::new(percpu_allocate);

    // Preemption count is percpu. So we need to initialize percpu area first.
    percpu_area.setup();

    eonix_preempt::disable();

    // SAFETY: Preemption is disabled.
    let mut cpu = unsafe { local_cpu() };

    unsafe {
        cpu.as_mut().init();
    }
    percpu_area.register(cpu.cpuid());

    eonix_preempt::enable();
}
