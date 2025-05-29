mod raw_page;

use super::{paging::AllocZeroed as _, Page};
use buddy_allocator::{BuddyAllocator, BuddyRawPage as _};
use core::{ptr::NonNull, sync::atomic::Ordering};
use eonix_mm::{
    address::{AddrOps as _, PAddr, PRange},
    paging::{GlobalPageAlloc as GlobalPageAllocTrait, PageAlloc, PFN},
};
use eonix_sync::{NoContext, Spin};
use intrusive_list::List;
use raw_page::{PageFlags, RawPagePtr};

pub use raw_page::RawPage;

const COSTLY_ORDER: u32 = 3;
const BATCH_SIZE: u32 = 64;

static BUDDY_ALLOC: Spin<BuddyAllocator<RawPagePtr>> = Spin::new(BuddyAllocator::new());

#[eonix_percpu::define_percpu]
static PERCPU_PAGE_ALLOC: PerCpuPageAlloc = PerCpuPageAlloc::new();

#[derive(Clone)]
pub struct GlobalPageAlloc;

#[derive(Clone)]
pub struct BuddyPageAlloc();

/// Allocator that allocates pages from the buddy allocator while we are still in
/// the early stage of the kernel when the preemption is both disabled and not functioning.
#[derive(Clone)]
pub struct EarlyPageAlloc();

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
            if let Some(pages_ptr) = BUDDY_ALLOC.lock().alloc_order(order) {
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
    #[allow(dead_code)]
    pub const fn buddy_alloc() -> BuddyPageAlloc {
        BuddyPageAlloc()
    }

    pub const fn early_alloc() -> EarlyPageAlloc {
        EarlyPageAlloc()
    }

    pub fn mark_present(range: PRange) {
        let mut pfn = PFN::from(range.start().ceil());
        let end_pfn = PFN::from(range.end().floor());

        while pfn < end_pfn {
            RawPagePtr::from(pfn).flags().set(PageFlags::PRESENT);
            pfn = pfn + 1;
        }
    }

    /// Add the pages in the PAddr range `range` to the global allocator.
    ///
    /// This function is only to be called on system initialization when `eonix_preempt`
    /// is not functioning due to the absence of percpu area.
    ///
    /// # Safety
    /// This function is unsafe because calling this function in preemptible context
    /// might involve dead locks.
    pub unsafe fn add_pages(range: PRange) {
        BUDDY_ALLOC
            .lock_with_context(NoContext)
            .create_pages(range.start(), range.end())
    }
}

impl PageAlloc for GlobalPageAlloc {
    type RawPage = RawPagePtr;

    fn alloc_order(&self, order: u32) -> Option<RawPagePtr> {
        if order > COSTLY_ORDER {
            BUDDY_ALLOC.lock().alloc_order(order)
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
            BUDDY_ALLOC.lock().dealloc(page_ptr);
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
        BuddyAllocator::has_management_over(page_ptr)
            && (page_ptr.order() > COSTLY_ORDER || page_ptr.flags().has(PageFlags::LOCAL))
    }
}

impl GlobalPageAllocTrait for GlobalPageAlloc {
    fn global() -> Self {
        GlobalPageAlloc
    }
}

impl PageAlloc for BuddyPageAlloc {
    type RawPage = RawPagePtr;

    fn alloc_order(&self, order: u32) -> Option<RawPagePtr> {
        BUDDY_ALLOC.lock().alloc_order(order)
    }

    unsafe fn dealloc(&self, page_ptr: RawPagePtr) {
        BUDDY_ALLOC.lock().dealloc(page_ptr);
    }

    fn has_management_over(&self, page_ptr: RawPagePtr) -> bool {
        BuddyAllocator::has_management_over(page_ptr)
    }
}

impl PageAlloc for EarlyPageAlloc {
    type RawPage = RawPagePtr;

    fn alloc_order(&self, order: u32) -> Option<Self::RawPage> {
        BUDDY_ALLOC.lock_with_context(NoContext).alloc_order(order)
    }

    unsafe fn dealloc(&self, raw_page: Self::RawPage) {
        BUDDY_ALLOC.lock_with_context(NoContext).dealloc(raw_page);
    }

    fn has_management_over(&self, page_ptr: Self::RawPage) -> bool {
        BuddyAllocator::has_management_over(page_ptr)
    }
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
