use super::{paging::AllocZeroed as _, Page};
use buddy_allocator::{BuddyAllocator, FreeArea as BuddyFreeArea};
use core::{ptr::NonNull, sync::atomic::Ordering};
use eonix_mm::{
    address::{AddrOps as _, PAddr},
    paging::{PageAlloc, PageFlags, RawPagePtr, PFN},
};

const COSTLY_ORDER: u32 = 3;
const BATCH_SIZE: u32 = 64;

#[arch::define_percpu]
static PERCPU_PAGE_ALLOC: PerCpuPageAlloc = PerCpuPageAlloc::new();

pub struct NoAlloc;

pub struct GlobalPageAlloc;

struct PerCpuPageAlloc {
    batch: u32,
    // TODO: might be used in the future.
    // high: u32,
    free_areas: [BuddyFreeArea; COSTLY_ORDER as usize + 1],
}

impl PerCpuPageAlloc {
    const fn new() -> Self {
        Self {
            batch: BATCH_SIZE,
            // high: 0,
            free_areas: [const { BuddyFreeArea::new() }; COSTLY_ORDER as usize + 1],
        }
    }

    fn do_alloc_order(&mut self, order: u32) -> Option<RawPagePtr> {
        assert!(order <= COSTLY_ORDER);
        let free_area = &mut self.free_areas[order as usize];

        let mut page_ptr = free_area.get_free_pages();

        if page_ptr.is_none() {
            let batch = self.batch >> order;
            for _ in 0..batch {
                if let Some(pages_ptr) = BuddyAllocator::alloc_order(order) {
                    pages_ptr.flags().set(PageFlags::LOCAL);
                    free_area.add_pages(pages_ptr);
                } else {
                    break;
                };
            }

            page_ptr = free_area.get_free_pages();
        }

        page_ptr.inspect(|page_ptr| page_ptr.flags().clear(PageFlags::FREE))
    }

    fn free_pages(&mut self, pages_ptr: RawPagePtr, order: u32) {
        assert_eq!(pages_ptr.order(), order);
        assert_eq!(pages_ptr.refcount().load(Ordering::Relaxed), 0);

        // TODO: Temporary workaround here.
        pages_ptr.refcount().store(1, Ordering::Relaxed);
        self.free_areas[order as usize].add_pages(pages_ptr);
    }
}

impl PageAlloc for GlobalPageAlloc {
    fn alloc_order(order: u32) -> Option<RawPagePtr> {
        if order > COSTLY_ORDER {
            BuddyAllocator::alloc_order(order)
        } else {
            PerCpuPageAlloc::alloc_order(order)
        }
    }

    unsafe fn dealloc(page_ptr: RawPagePtr) {
        if page_ptr.order() > COSTLY_ORDER {
            BuddyAllocator::dealloc(page_ptr);
        } else {
            PerCpuPageAlloc::dealloc(page_ptr);
        }
    }

    unsafe fn has_management_over(page_ptr: RawPagePtr) -> bool {
        if page_ptr.order() > COSTLY_ORDER {
            BuddyAllocator::has_management_over(page_ptr)
        } else {
            PerCpuPageAlloc::has_management_over(page_ptr)
        }
    }
}

impl PageAlloc for NoAlloc {
    fn alloc_order(_order: u32) -> Option<RawPagePtr> {
        panic!("NoAlloc cannot allocate pages");
    }

    unsafe fn dealloc(_: RawPagePtr) {
        panic!("NoAlloc cannot deallocate pages");
    }

    unsafe fn has_management_over(_: RawPagePtr) -> bool {
        true
    }
}

impl PageAlloc for PerCpuPageAlloc {
    fn alloc_order(order: u32) -> Option<RawPagePtr> {
        let page_ptr;
        unsafe {
            eonix_preempt::disable();
            page_ptr = PERCPU_PAGE_ALLOC.as_mut().do_alloc_order(order);
            eonix_preempt::enable();
        }

        page_ptr
    }

    unsafe fn dealloc(page_ptr: RawPagePtr) {
        let order = page_ptr.order();

        unsafe {
            eonix_preempt::disable();
            PERCPU_PAGE_ALLOC.as_mut().free_pages(page_ptr, order);
            eonix_preempt::enable();
        }
    }

    unsafe fn has_management_over(page_ptr: RawPagePtr) -> bool {
        BuddyAllocator::has_management_over(page_ptr) && page_ptr.flags().has(PageFlags::LOCAL)
    }
}

#[no_mangle]
pub extern "C" fn mark_present(start: usize, end: usize) {
    let mut start_pfn = PFN::from(PAddr::from(start).ceil());
    let end_pfn = PFN::from(PAddr::from(end).floor());

    while start_pfn < end_pfn {
        RawPagePtr::from(start_pfn).flags().set(PageFlags::PRESENT);
        start_pfn = start_pfn + 1;
    }
}

#[no_mangle]
pub extern "C" fn create_pages(start: PAddr, end: PAddr) {
    BuddyAllocator::create_pages(start, end);
}

#[no_mangle]
pub extern "C" fn page_to_pfn(page: *const ()) -> PFN {
    let page_ptr = RawPagePtr::new(NonNull::new(page as *mut _).unwrap());
    PFN::from(page_ptr)
}

#[no_mangle]
pub extern "C" fn c_alloc_page() -> *const () {
    GlobalPageAlloc::alloc().expect("Out of memory").as_ptr() as *const _
}

#[no_mangle]
pub extern "C" fn c_alloc_pages(order: u32) -> *const () {
    GlobalPageAlloc::alloc_order(order)
        .expect("Out of memory")
        .as_ptr() as *const _
}

#[no_mangle]
pub extern "C" fn c_alloc_page_table() -> PAddr {
    PAddr::from(Page::zeroed().into_raw())
}
