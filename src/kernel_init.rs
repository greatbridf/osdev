use eonix_hal::arch_exported::mm::with_global_page_table;
use eonix_hal::bootstrap::BootStrapData;
use eonix_hal::mm::{ArchMemory, BasicPageAllocRef};
use eonix_hal::traits::mm::Memory;
use eonix_mm::address::{AddrOps as _, VAddr, VRange};
use eonix_mm::page_table::PageAttribute;
use eonix_mm::paging::{Folio as _, FrameAlloc, PAGE_SIZE, PFN};
use streaming_iterator::StreamingIterator as _;

use crate::kernel::mem::{GlobalPageAlloc, PageReal};

fn setup_kernel_page_array(alloc: BasicPageAllocRef, count_pages: usize) {
    // Map kernel page array.
    // TODO: This should be done by the global Zone
    const V_KERNEL_PAGE_ARRAY_START: VAddr = VAddr::from(0xffffff8040000000);

    let range =
        VRange::from(V_KERNEL_PAGE_ARRAY_START).grow(PAGE_SIZE * count_pages);

    with_global_page_table(alloc.clone(), |table| {
        let mut ptes = table.iter_kernel(range);
        while let Some(pte) = ptes.next() {
            let attr = PageAttribute::PRESENT
                | PageAttribute::WRITE
                | PageAttribute::READ
                | PageAttribute::GLOBAL
                | PageAttribute::ACCESSED
                | PageAttribute::DIRTY;

            let page = alloc.alloc().unwrap();
            pte.set(page.into_raw(), attr);
        }
    });

    // TODO!!!: Construct the global zone with all present ram.
    // for range in ArchMemory::present_ram() {
    //     GlobalPageAlloc::mark_present(range);
    // }

    unsafe {
        // SAFETY: We've just mapped the area with sufficient length.
        core::ptr::write_bytes(
            V_KERNEL_PAGE_ARRAY_START.addr() as *mut u8,
            0,
            count_pages * PAGE_SIZE,
        );
    }
}

pub fn setup_memory(data: &mut BootStrapData) {
    let addr_max = ArchMemory::present_ram()
        .map(|range| range.end())
        .max()
        .expect("No free memory");

    let pfn_max = PFN::from(addr_max.ceil());
    let len_bytes_page_array = usize::from(pfn_max) * size_of::<PageReal>();
    let count_pages = len_bytes_page_array.div_ceil(PAGE_SIZE);

    let alloc = data.get_alloc().unwrap();
    setup_kernel_page_array(alloc, count_pages);

    if let Some(early_alloc) = data.take_alloc() {
        for range in early_alloc.into_iter() {
            unsafe {
                // SAFETY: We are in system initialization procedure where preemption is disabled.
                GlobalPageAlloc::add_pages(range);
            }
        }
    }
}
