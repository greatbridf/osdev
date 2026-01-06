mod raw_page;
mod zones;

use core::sync::atomic::Ordering;

use buddy_allocator::BuddyAllocator;
use eonix_mm::address::{AddrOps as _, PRange};
use eonix_mm::paging::{
    GlobalPageAlloc as GlobalPageAllocTrait, PageAlloc, PageList, PageListSized as _, PFN,
};
use eonix_preempt::PreemptGuard;
use eonix_sync::{NoContext, Spin};
use raw_page::{PageFlags, RawPageList};
pub use raw_page::{RawPage, RawPagePtr};
pub use zones::GlobalZone;

const COSTLY_ORDER: u32 = 3;
const AREAS: usize = COSTLY_ORDER as usize + 1;
const BATCH_SIZE: u32 = 64;

static BUDDY_ALLOC: Spin<BuddyAllocator<GlobalZone, RawPageList>> =
    Spin::new(BuddyAllocator::new(&GlobalZone()));

#[eonix_percpu::define_percpu]
static PERCPU_PAGE_ALLOC: PerCpuPageAlloc = PerCpuPageAlloc::new();

#[derive(Clone)]
pub struct GlobalPageAlloc;

#[derive(Clone)]
pub struct BuddyPageAlloc();

struct PerCpuPageAlloc {
    batch: u32,
    free_areas: [RawPageList; AREAS],
}

pub trait PerCpuPage {
    fn set_local(&mut self, val: bool);
}

impl PerCpuPageAlloc {
    const fn new() -> Self {
        Self {
            batch: BATCH_SIZE,
            free_areas: [RawPageList::NEW; AREAS],
        }
    }

    fn alloc_order(&mut self, order: u32) -> Option<&'static mut RawPage> {
        assert!(order <= COSTLY_ORDER);
        if let Some(pages) = self.free_areas[order as usize].pop_head() {
            return Some(pages);
        }

        let batch = self.batch >> order;
        for _ in 0..batch {
            let Some(page) = BUDDY_ALLOC.lock().alloc_order(order) else {
                break;
            };

            page.set_local(true);
            self.free_areas[order as usize].push_tail(page);
        }

        self.free_areas[order as usize].pop_head()
    }

    fn free_pages(&mut self, page: &'static mut RawPage, order: u32) {
        self.free_areas[order as usize].push_tail(page);
    }
}

impl GlobalPageAlloc {
    #[allow(dead_code)]
    pub const fn buddy_alloc() -> BuddyPageAlloc {
        BuddyPageAlloc()
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
        let raw_page = if order > COSTLY_ORDER {
            BUDDY_ALLOC.lock().alloc_order(order)
        } else {
            unsafe {
                eonix_preempt::disable();
                let page = PERCPU_PAGE_ALLOC.as_mut().alloc_order(order);
                eonix_preempt::enable();

                page
            }
        };

        raw_page.map(|raw_page| {
            // SAFETY: Memory order here can be Relaxed is for the same reason
            //         as that in the copy constructor of `std::shared_ptr`.
            raw_page.refcount.fetch_add(1, Ordering::Relaxed);

            RawPagePtr::from_ref(raw_page)
        })
    }

    unsafe fn dealloc(&self, page_ptr: RawPagePtr) {
        assert_eq!(
            page_ptr.refcount().load(Ordering::Relaxed),
            0,
            "Trying to free a page with refcount > 0"
        );

        if page_ptr.order() > COSTLY_ORDER {
            BUDDY_ALLOC.lock().dealloc(page_ptr.as_mut());
        } else {
            let order = page_ptr.order();

            unsafe {
                PreemptGuard::new(PERCPU_PAGE_ALLOC.as_mut()).free_pages(page_ptr.as_mut(), order);
            }
        }
    }

    fn has_management_over(&self, page_ptr: RawPagePtr) -> bool {
        page_ptr.order() > COSTLY_ORDER || page_ptr.flags().has(PageFlags::LOCAL)
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
        BUDDY_ALLOC
            .lock()
            .alloc_order(order)
            .map(|raw_page| RawPagePtr::from_ref(raw_page))
    }

    unsafe fn dealloc(&self, page_ptr: RawPagePtr) {
        BUDDY_ALLOC.lock().dealloc(page_ptr.as_mut());
    }

    fn has_management_over(&self, _: RawPagePtr) -> bool {
        true
    }
}
