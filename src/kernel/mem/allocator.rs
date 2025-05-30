use core::alloc::{GlobalAlloc, Layout};
use eonix_mm::paging::{PageAlloc, PAGE_SIZE_BITS};
use eonix_sync::LazyLock;
use slab_allocator::{SlabAllocator, SlabRawPage};

use super::page_alloc::RawPagePtr;
use super::GlobalPageAlloc;

static SLAB_ALLOCATOR: LazyLock<SlabAllocator<RawPagePtr, GlobalPageAlloc, 9>> =
    LazyLock::new(|| SlabAllocator::new_in(GlobalPageAlloc));

struct Allocator;

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size().next_power_of_two();

        let result = if size <= 2048 {
            SLAB_ALLOCATOR.alloc(size)
        } else {
            let page_num = size >> PAGE_SIZE_BITS;
            let order = page_num.next_power_of_two().trailing_zeros();
            let raw_page = GlobalPageAlloc
                .alloc_order(order)
                .expect("allocate page failed!");
            raw_page.real_ptr().as_ptr()
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
            let page_ptr: RawPagePtr = SlabRawPage::in_which(ptr);
            page_ptr
                .as_mut()
                .refcount
                .fetch_sub(1, core::sync::atomic::Ordering::Relaxed);
            GlobalPageAlloc.dealloc(page_ptr);
        };
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
