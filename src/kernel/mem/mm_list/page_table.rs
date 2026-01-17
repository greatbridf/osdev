use core::ops::Deref;

use eonix_hal::arch_exported::mm::{ArchPagingMode, PageAccessImpl};
use eonix_hal::mm::GLOBAL_PAGE_TABLE;
use eonix_mm::page_table::PageTable;
use eonix_mm::paging::{Folio, GlobalFrameAlloc};

use crate::kernel::mem::{FolioOwned, GlobalPageAlloc, PhysAccess};

#[repr(transparent)]
pub struct KernelPageTable(PageTable<'static, ArchPagingMode, GlobalPageAlloc, PageAccessImpl>);

impl KernelPageTable {
    pub fn new() -> Self {
        let global_page_table = unsafe {
            // SAFETY: The region is valid and read only after initialization.
            GLOBAL_PAGE_TABLE.start().as_ptr::<[u8; 4096]>().as_ref()
        };

        let mut table_page = FolioOwned::alloc();
        let entries = table_page.as_bytes_mut().len();
        table_page.as_bytes_mut()[..(entries / 2)].fill(0);
        table_page.as_bytes_mut()[(entries / 2)..]
            .copy_from_slice(&global_page_table[(entries / 2)..]);

        Self(PageTable::new(
            table_page.share(),
            GlobalPageAlloc::GLOBAL,
            PageAccessImpl,
        ))
    }
}

impl Deref for KernelPageTable {
    type Target = PageTable<'static, ArchPagingMode, GlobalPageAlloc, PageAccessImpl>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
