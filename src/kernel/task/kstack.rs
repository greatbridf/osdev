use arch::InterruptContext;

use crate::kernel::mem::{
    paging::Page,
    phys::{CachedPP, PhysPtr},
};

pub struct KernelStack {
    pages: Page,
    bottom: usize,
}

unsafe extern "C" fn __not_assigned_entry() {
    panic!("__not_assigned_entry called");
}

impl KernelStack {
    /// Kernel stack page order
    /// 7 for `2^7 = 128 pages = 512 KiB`
    const KERNEL_STACK_ORDER: u32 = 7;

    pub fn new() -> Self {
        let pages = Page::alloc_many(Self::KERNEL_STACK_ORDER);
        let bottom = pages.as_cached().offset(pages.len()).as_ptr::<u8>() as usize;

        Self {
            pages,
            bottom,
        }
    }

    pub fn load_interrupt_stack(&self) {
        const TSS_RSP0: CachedPP = CachedPP::new(0x00000074);

        // TODO!!!: Make `TSS` a per cpu struct.
        // SAFETY: `TSS_RSP0` is always valid.
        unsafe {
            TSS_RSP0.as_ptr::<u64>().write_unaligned(self.bottom as u64);
        }
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
