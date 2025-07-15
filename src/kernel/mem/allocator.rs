use super::page_alloc::RawPagePtr;
use super::{AsMemoryBlock, GlobalPageAlloc, Page};
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;
use eonix_hal::mm::ArchPhysAccess;
use eonix_mm::address::PhysAccess;
use eonix_mm::paging::{PAGE_SIZE_BITS, PFN};
use eonix_sync::LazyLock;
use slab_allocator::SlabAllocator;

static SLAB_ALLOCATOR: LazyLock<SlabAllocator<RawPagePtr, GlobalPageAlloc, 9>> =
    LazyLock::new(|| SlabAllocator::new_in(GlobalPageAlloc));

struct Allocator;

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size().next_power_of_two();

        let result = if size <= 2048 {
            SLAB_ALLOCATOR.alloc(size)
        } else {
            let page_count = size >> PAGE_SIZE_BITS;
            let page = Page::alloc_at_least(page_count);

            let ptr = page.as_memblk().as_ptr();
            page.into_raw();

            ptr.as_ptr()
        };

        if result.is_null() {
            core::ptr::null_mut()
        } else {
            result as *mut u8
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size().next_power_of_two();

        if size <= 2048 {
            SLAB_ALLOCATOR.dealloc(ptr, size)
        } else {
            let paddr = ArchPhysAccess::from_ptr(NonNull::new_unchecked(ptr));
            let pfn = PFN::from(paddr);
            Page::from_raw(pfn);
        };
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
