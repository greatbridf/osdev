mod raw_page;

use super::{paging::AllocZeroed as _, Page};
use buddy_allocator::{BuddyAllocator, BuddyRawPage as _};
use core::{ptr::NonNull, sync::atomic::Ordering};
use eonix_mm::{
    address::{AddrOps as _, PAddr},
    paging::{GlobalPageAlloc as GlobalPageAllocTrait, PageAlloc, PFN},
};
use intrusive_list::List;
use raw_page::{PageFlags, RawPage, RawPagePtr};

const COSTLY_ORDER: u32 = 3;
const BATCH_SIZE: u32 = 64;

static REAL_BUDDY_ALLOC: BuddyAllocator<RawPagePtr> = BuddyAllocator::new();
static BUDDY_ALLOC: &'static BuddyAllocator<RawPagePtr> = &REAL_BUDDY_ALLOC;

#[arch::define_percpu]
static PERCPU_PAGE_ALLOC: PerCpuPageAlloc = PerCpuPageAlloc::new();

#[derive(Clone)]
pub struct NoAlloc;

#[derive(Clone)]
pub struct GlobalPageAlloc;

struct PerCpuPageAlloc {
    batch: u32,
    // TODO: might be used in the future.
    // high: u32,
    free_areas: [List; COSTLY_ORDER as usize + 1],
}

impl PerCpuPageAlloc {
    const fn new() -> Self {
        Self {
            batch: BATCH_SIZE,
            // high: 0,
            free_areas: [const { List::new() }; COSTLY_ORDER as usize + 1],
        }
    }

    fn insert_free_pages(&mut self, pages_ptr: RawPagePtr, order: u32) {
        let free_area = &mut self.free_areas[order as usize];
        free_area.insert(unsafe { pages_ptr.get_link() });
    }

    fn get_free_pages(&mut self, order: u32) -> Option<RawPagePtr> {
        let free_area = &mut self.free_areas[order as usize];
        free_area.pop().map(|node| unsafe {
            // SAFETY: `node` is a valid pointer to a `Link` that is not used by anyone.
            RawPagePtr::from_link(node)
        })
    }

    fn alloc_order(&mut self, order: u32) -> Option<RawPagePtr> {
        assert!(order <= COSTLY_ORDER);
        if let Some(pages) = self.get_free_pages(order) {
            return Some(pages);
        }

        let batch = self.batch >> order;
        for _ in 0..batch {
            if let Some(pages_ptr) = BUDDY_ALLOC.alloc_order(order) {
                pages_ptr.flags().set(PageFlags::LOCAL);
                self.insert_free_pages(pages_ptr, order);
            } else {
                break;
            };
        }

        self.get_free_pages(order)
    }

    fn free_pages(&mut self, pages_ptr: RawPagePtr, order: u32) {
        assert_eq!(pages_ptr.order(), order);
        assert_eq!(pages_ptr.refcount().load(Ordering::Relaxed), 0);

        pages_ptr.refcount().store(1, Ordering::Relaxed);
        self.insert_free_pages(pages_ptr, order);
    }
}

impl GlobalPageAlloc {
    pub const fn buddy_alloc() -> &'static BuddyAllocator<RawPagePtr> {
        BUDDY_ALLOC
    }
}

impl PageAlloc for GlobalPageAlloc {
    type RawPage = RawPagePtr;

    fn alloc_order(&self, order: u32) -> Option<RawPagePtr> {
        if order > COSTLY_ORDER {
            BUDDY_ALLOC.alloc_order(order)
        } else {
            unsafe {
                eonix_preempt::disable();
                let page_ptr = PERCPU_PAGE_ALLOC.as_mut().alloc_order(order);
                eonix_preempt::enable();
                page_ptr
            }
        }
    }

    unsafe fn dealloc(&self, page_ptr: RawPagePtr) {
        if page_ptr.order() > COSTLY_ORDER {
            BUDDY_ALLOC.dealloc(page_ptr);
        } else {
            let order = page_ptr.order();
            unsafe {
                eonix_preempt::disable();
                PERCPU_PAGE_ALLOC.as_mut().free_pages(page_ptr, order);
                eonix_preempt::enable();
            }
        }
    }

    fn has_management_over(&self, page_ptr: RawPagePtr) -> bool {
        BUDDY_ALLOC.has_management_over(page_ptr)
            && (page_ptr.order() > COSTLY_ORDER || page_ptr.flags().has(PageFlags::LOCAL))
    }
}

impl PageAlloc for NoAlloc {
    type RawPage = RawPagePtr;

    fn alloc_order(&self, _order: u32) -> Option<RawPagePtr> {
        panic!("NoAlloc cannot allocate pages");
    }

    unsafe fn dealloc(&self, _: RawPagePtr) {
        panic!("NoAlloc cannot deallocate pages");
    }

    fn has_management_over(&self, _: RawPagePtr) -> bool {
        true
    }
}

impl GlobalPageAllocTrait for GlobalPageAlloc {
    fn global() -> Self {
        GlobalPageAlloc
    }
}

impl GlobalPageAllocTrait for NoAlloc {
    fn global() -> Self {
        NoAlloc
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
    REAL_BUDDY_ALLOC.create_pages(start, end);
}

#[no_mangle]
pub extern "C" fn page_to_pfn(page: *const ()) -> PFN {
    let page_ptr = RawPagePtr::new(NonNull::new(page as *mut _).unwrap());
    PFN::from(page_ptr)
}

#[no_mangle]
pub extern "C" fn c_alloc_page() -> *const RawPage {
    GlobalPageAlloc.alloc().expect("Out of memory").as_ref()
}

#[no_mangle]
pub extern "C" fn c_alloc_pages(order: u32) -> *const RawPage {
    GlobalPageAlloc
        .alloc_order(order)
        .expect("Out of memory")
        .as_ref()
}

#[no_mangle]
pub extern "C" fn c_alloc_page_table() -> PAddr {
    PAddr::from(Page::zeroed().into_raw())
}
