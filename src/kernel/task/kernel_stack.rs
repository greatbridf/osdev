use core::ptr::NonNull;

use eonix_hal::traits::trap::Stack;

use crate::kernel::mem::FolioOwned;

#[derive(Debug)]
pub struct KernelStack {
    folio: FolioOwned,
}

impl KernelStack {
    /// Kernel stack page order
    /// 7 for `2^7 = 128 pages = 512 KiB`
    const KERNEL_STACK_ORDER: u32 = 7;

    pub fn new() -> Self {
        Self {
            folio: FolioOwned::alloc_order(Self::KERNEL_STACK_ORDER),
        }
    }

    pub fn get_bottom(&self) -> NonNull<()> {
        let ptr = self.folio.get_bytes_ptr();
        let len = ptr.len();

        // SAFETY: The vaddr of the folio is guaranteed to be non-zero.
        unsafe { ptr.cast().byte_add(len) }
    }
}

impl Stack for KernelStack {
    fn get_bottom(&self) -> *mut usize {
        self.get_bottom().cast().as_ptr()
    }
}
