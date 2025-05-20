use crate::kernel::mem::{paging::Page, PhysAccess as _};
use core::{num::NonZero, ptr::NonNull};
use eonix_runtime::executor::Stack;

#[derive(Debug)]
pub struct KernelStack {
    _pages: Page,
    bottom: NonZero<usize>,
}

impl KernelStack {
    /// Kernel stack page order
    /// 7 for `2^7 = 128 pages = 512 KiB`
    const KERNEL_STACK_ORDER: u32 = 7;

    pub fn new() -> Self {
        let pages = Page::alloc_order(Self::KERNEL_STACK_ORDER);
        let bottom = unsafe {
            // SAFETY: The paddr is from a page, which should be valid.
            pages.range().end().as_ptr::<u8>().addr()
        };

        Self {
            _pages: pages,
            bottom,
        }
    }
}

impl Stack for KernelStack {
    fn new() -> Self {
        Self::new()
    }

    fn get_bottom(&self) -> NonNull<()> {
        // SAFETY: The stack is allocated and `bottom` is non-zero.
        unsafe { NonNull::new_unchecked(self.bottom.get() as *mut _) }
    }
}
