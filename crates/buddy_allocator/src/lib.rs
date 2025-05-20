#![no_std]

mod free_area;
mod zone;

use core::sync::atomic::Ordering;
use eonix_mm::{
    address::PAddr,
    paging::{PageAlloc, PageFlags, RawPagePtr, PFN},
};
use eonix_sync::Spin;
use zone::Zone;

pub use free_area::FreeArea;

const MAX_ORDER: u32 = 10;
const ZONE_AREAS: usize = const { MAX_ORDER as usize + 1 };

static BUDDY_ALLOCATOR: BuddyAllocator = BuddyAllocator::new();

pub struct BuddyAllocator {
    zone: Spin<Zone<ZONE_AREAS>>,
}

impl BuddyAllocator {
    const fn new() -> Self {
        Self {
            zone: Spin::new(Zone::new()),
        }
    }

    pub fn create_pages(start: PAddr, end: PAddr) {
        BUDDY_ALLOCATOR.zone.lock().create_pages(start, end);
    }
}

impl PageAlloc for BuddyAllocator {
    fn alloc_order(order: u32) -> Option<RawPagePtr> {
        let pages_ptr = BUDDY_ALLOCATOR.zone.lock().get_free_pages(order);

        if let Some(pages_ptr) = pages_ptr {
            // SAFETY: Memory order here can be Relaxed is for the same reason as that
            // in the copy constructor of `std::shared_ptr`.
            pages_ptr.refcount().fetch_add(1, Ordering::Relaxed);
            pages_ptr.flags().clear(PageFlags::FREE);
        }

        pages_ptr
    }

    unsafe fn dealloc(page_ptr: RawPagePtr) {
        BUDDY_ALLOCATOR.zone.lock().free_pages(page_ptr);
    }

    unsafe fn has_management_over(page_ptr: RawPagePtr) -> bool {
        !page_ptr.flags().has(PageFlags::FREE) && page_ptr.flags().has(PageFlags::BUDDY)
    }
}

pub(self) trait BuddyPFNOps {
    fn buddy_pfn(self, order: u32) -> PFN;
    fn combined_pfn(self, buddy_pfn: PFN) -> PFN;
}

impl BuddyPFNOps for PFN {
    fn buddy_pfn(self, order: u32) -> PFN {
        PFN::from(usize::from(self) ^ (1 << order))
    }

    fn combined_pfn(self, buddy_pfn: PFN) -> PFN {
        PFN::from(usize::from(self) & usize::from(buddy_pfn))
    }
}
