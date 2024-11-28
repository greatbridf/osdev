use crate::kernel::{
    cpu::current_cpu,
    mem::{paging::Page, phys::PhysPtr},
};
use arch::InterruptContext;

pub struct KernelStack {
    pages: Page,
    bottom: usize,
}

impl KernelStack {
    /// Kernel stack page order
    /// 7 for `2^7 = 128 pages = 512 KiB`
    const KERNEL_STACK_ORDER: u32 = 7;

    pub fn new() -> Self {
        let pages = Page::alloc_many(Self::KERNEL_STACK_ORDER);
        let bottom = pages.as_cached().offset(pages.len()).as_ptr::<u8>() as usize;

        Self { pages, bottom }
    }

    /// # Safety
    /// This function is unsafe because it accesses the `current_cpu()`, which needs
    /// to be called in a preemption disabled context.
    pub unsafe fn load_interrupt_stack(&self) {
        arch::load_interrupt_stack(current_cpu(), self.bottom as u64);
    }

    pub fn get_stack_bottom(&self) -> usize {
        self.bottom
    }

    pub fn init(&self, interrupt_context: InterruptContext) -> usize {
        let mut sp = self.bottom - core::mem::size_of::<InterruptContext>();
        sp &= !0xf;
        unsafe {
            (sp as *mut InterruptContext).write(interrupt_context);
        }
        sp
    }
}
