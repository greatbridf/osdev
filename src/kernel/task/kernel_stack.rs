use crate::kernel::mem::{paging::Page, phys::PhysPtr};
use eonix_runtime::executor::Stack;

#[derive(Debug)]
pub struct KernelStack {
    _pages: Page,
    bottom: usize,
}

impl KernelStack {
    /// Kernel stack page order
    /// 7 for `2^7 = 128 pages = 512 KiB`
    const KERNEL_STACK_ORDER: u32 = 7;

    pub fn new() -> Self {
        let pages = Page::alloc_many(Self::KERNEL_STACK_ORDER);
        let bottom = pages.as_cached().offset(pages.len()).as_ptr::<u8>() as usize;

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

    fn get_bottom(&self) -> &() {
        // SAFETY: We hold the ownership of a valid stack.
        unsafe { &*(self.bottom as *const ()) }
    }
}
