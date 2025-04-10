use super::mem::{paging::Page, phys::PhysPtr as _};
use arch::{PercpuArea, CPU};
use core::{alloc::Layout, mem::ManuallyDrop, pin::Pin, ptr::NonNull};
use eonix_sync::LazyLock;

#[arch::define_percpu]
static CPU: LazyLock<CPU> = LazyLock::new(CPU::new);

/// # Safety
/// This function is unsafe because it needs preemption to be disabled.
pub unsafe fn local_cpu() -> Pin<&'static mut CPU> {
    // SAFETY: `CPU_STATUS` is global static and initialized only once.
    unsafe { Pin::new_unchecked(CPU.as_mut().get_mut()) }
}

pub fn percpu_allocate(layout: Layout) -> NonNull<u8> {
    // TODO: Use page size defined in `arch`.
    let page_count = layout.size().div_ceil(arch::PAGE_SIZE);
    let page = ManuallyDrop::new(Page::early_alloc_ceil(page_count));
    let pointer = page.as_cached().as_ptr();

    NonNull::new(pointer).expect("Allocated page pfn should be non-null.")
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
