use crate::kernel::mem::{GlobalPageAlloc, RawPage};
use eonix_hal::{
    bootstrap::BootStrapData,
    mm::{ArchMemory, ArchPagingMode, GLOBAL_PAGE_TABLE},
    traits::mm::Memory,
};
use eonix_mm::{
    address::{Addr as _, AddrOps as _, VAddr, VRange},
    page_table::{PageAttribute, PagingMode as _, PTE},
    paging::{Page as GenericPage, PAGE_SIZE, PFN},
};

pub fn setup_memory(data: &mut BootStrapData) {
    let addr_max = ArchMemory::present_ram()
        .map(|range| range.end())
        .max()
        .expect("No free memory");

    let pfn_max = PFN::from(addr_max.ceil());
    let len_bytes_page_array = usize::from(pfn_max) * size_of::<RawPage>();
    let count_pages = len_bytes_page_array.div_ceil(PAGE_SIZE);

    let alloc = data.get_alloc().unwrap();

    // Map kernel page array.
    const V_KERNEL_PAGE_ARRAY_START: VAddr = VAddr::from(0xffffff8040000000);

    for pte in GLOBAL_PAGE_TABLE.iter_kernel_in(
        VRange::from(V_KERNEL_PAGE_ARRAY_START).grow(PAGE_SIZE * count_pages),
        ArchPagingMode::LEVELS,
        &alloc,
    ) {
        let attr = PageAttribute::PRESENT
            | PageAttribute::WRITE
            | PageAttribute::READ
            | PageAttribute::GLOBAL
            | PageAttribute::ACCESSED
            | PageAttribute::DIRTY;

        let page = GenericPage::alloc_in(&alloc);
        pte.set(page.into_raw(), attr.into());
    }

    unsafe {
        // SAFETY: We've just mapped the area with sufficient length.
        core::ptr::write_bytes(
            V_KERNEL_PAGE_ARRAY_START.addr() as *mut (),
            0,
            count_pages * PAGE_SIZE,
        );
    }

    for range in ArchMemory::present_ram() {
        GlobalPageAlloc::mark_present(range);
    }

    if let Some(early_alloc) = data.take_alloc() {
        for range in early_alloc.into_iter() {
            unsafe {
                // SAFETY: We are in system initialization procedure where preemption is disabled.
                GlobalPageAlloc::add_pages(range);
            }
        }
    }
}
