mod raw_page;
mod zones;

use core::sync::atomic::Ordering;

use buddy_allocator::BuddyAllocator;
use eonix_mm::address::PRange;
use eonix_mm::page_table::PageTableAlloc;
use eonix_mm::paging::{FolioList, FolioListSized as _, FrameAlloc, GlobalFrameAlloc, PFN};
use eonix_preempt::PreemptGuard;
use eonix_sync::{NoContext, Spin};
pub use raw_page::{PageFlags, RawPage, RawPageList};
pub use zones::{GlobalZone, ZONE};

use super::folio::Folio;

const COSTLY_ORDER: u32 = 3;
const AREAS: usize = COSTLY_ORDER as usize + 1;
const BATCH_SIZE: u32 = 64;

static BUDDY_ALLOC: Spin<BuddyAllocator<GlobalZone, RawPageList>> =
    Spin::new(BuddyAllocator::new(&GlobalZone()));

#[eonix_percpu::define_percpu]
static PERCPU_PAGE_ALLOC: PerCpuPageAlloc = PerCpuPageAlloc::new();

#[derive(Clone)]
pub struct GlobalPageAlloc;

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
            .create_folios(range.start(), range.end())
    }

    pub fn alloc_raw_order(&self, order: u32) -> Option<&'static mut RawPage> {
        if order > COSTLY_ORDER {
            BUDDY_ALLOC.lock().alloc_order(order)
        } else {
            unsafe {
                eonix_preempt::disable();
                let page = PERCPU_PAGE_ALLOC.as_mut().alloc_order(order);
                eonix_preempt::enable();

                page
            }
        }
    }

    pub unsafe fn dealloc_raw(&self, raw_page: &'static mut RawPage) {
        assert_eq!(
            raw_page.refcount.load(Ordering::Relaxed),
            0,
            "Trying to free a page with refcount > 0"
        );

        if raw_page.order > COSTLY_ORDER {
            BUDDY_ALLOC.lock().dealloc(raw_page);
        } else {
            let order = raw_page.order;

            unsafe {
                PreemptGuard::new(PERCPU_PAGE_ALLOC.as_mut()).free_pages(raw_page, order);
            }
        }
    }
}

impl FrameAlloc for GlobalPageAlloc {
    type Folio = Folio;

    fn alloc_order(&self, order: u32) -> Option<Self::Folio> {
        self.alloc_raw_order(order).map(|raw_page| {
            // SAFETY: Memory order here can be Relaxed is for the same reason
            //         as that in the copy constructor of `std::shared_ptr`.

            raw_page.refcount.fetch_add(1, Ordering::Relaxed);
            Folio::from_mut_page(raw_page)
        })
    }
}

impl GlobalFrameAlloc for GlobalPageAlloc {
    const GLOBAL: Self = GlobalPageAlloc;
}

impl PageTableAlloc for GlobalPageAlloc {
    type Folio = Folio;

    fn alloc(&self) -> Self::Folio {
        FrameAlloc::alloc(self).unwrap()
    }

    unsafe fn from_raw(&self, pfn: PFN) -> Self::Folio {
        unsafe { Folio::from_raw(pfn) }
    }
}
