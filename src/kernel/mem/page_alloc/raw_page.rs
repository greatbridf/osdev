use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use buddy_allocator::BuddyFolio;
use eonix_hal::mm::ArchPhysAccess;
use eonix_mm::address::{PAddr, PhysAccess as _};
use eonix_mm::paging::{FolioList, FolioListSized, Zone, PFN};
use intrusive_list::{container_of, Link, List};
use slab_allocator::{SlabPage, SlabPageAlloc, SlabSlot};

use super::zones::ZONE;
use super::{GlobalPageAlloc, PerCpuPage};
use crate::kernel::mem::PhysAccess;

pub struct PageFlags(AtomicU32);

#[derive(Clone, Copy)]
struct SlabPageData {
    allocated_count: usize,
    free_next: Option<NonNull<SlabSlot>>,
}

impl SlabPageData {
    const fn new() -> Self {
        Self {
            allocated_count: 0,
            free_next: None,
        }
    }
}

#[repr(C)]
union PageData {
    slab: SlabPageData,
}

pub struct RawPage {
    /// This can be used for LRU page swap in the future.
    ///
    /// Now only used for free page links in the buddy system.
    pub link: Link,
    /// # Safety
    /// This field is only used in buddy system and is protected by the global lock.
    pub order: u32,
    pub flags: PageFlags,
    pub refcount: AtomicUsize,

    shared_data: PageData,
}

// XXX: introduce Folio and remove this.
unsafe impl Send for RawPage {}
unsafe impl Sync for RawPage {}

impl PageFlags {
    pub const LOCKED: u32 = 1 << 1;
    pub const BUDDY: u32 = 1 << 2;
    pub const SLAB: u32 = 1 << 3;
    pub const DIRTY: u32 = 1 << 4;
    pub const LOCAL: u32 = 1 << 6;

    pub fn has(&self, flag: u32) -> bool {
        (self.0.load(Ordering::Relaxed) & flag) == flag
    }

    pub fn set(&self, flag: u32) {
        self.0.fetch_or(flag, Ordering::Relaxed);
    }

    pub fn clear(&self, flag: u32) {
        self.0.fetch_and(!flag, Ordering::Relaxed);
    }

    /// Set the flag and return whether it was already set.
    ///
    /// If multiple flags are given, returns true if any of them were already set.
    pub fn test_and_set(&self, flag: u32) -> bool {
        (self.0.fetch_or(flag, Ordering::Relaxed) & flag) != 0
    }
}

impl BuddyFolio for RawPage {
    fn pfn(&self) -> PFN {
        ZONE.get_pfn(self)
    }

    fn get_order(&self) -> u32 {
        self.order
    }

    fn is_buddy(&self) -> bool {
        self.flags.has(PageFlags::BUDDY)
    }

    fn set_order(&mut self, order: u32) {
        self.order = order;
    }

    fn set_buddy(&mut self, val: bool) {
        if val {
            self.flags.set(PageFlags::BUDDY);
        } else {
            self.flags.clear(PageFlags::BUDDY)
        }
    }
}

impl SlabPage for RawPage {
    fn get_data_ptr(&self) -> NonNull<[u8]> {
        let paddr_start = PAddr::from(ZONE.get_pfn(self));
        let page_data_ptr = unsafe { paddr_start.as_ptr() };

        NonNull::slice_from_raw_parts(page_data_ptr, 1 << (self.order + 12))
    }

    fn get_free_slot(&self) -> Option<NonNull<SlabSlot>> {
        unsafe {
            // SAFETY: TODO
            self.shared_data.slab.free_next
        }
    }

    fn set_free_slot(&mut self, next: Option<NonNull<SlabSlot>>) {
        self.shared_data.slab.free_next = next;
    }

    fn get_alloc_count(&self) -> usize {
        unsafe {
            // SAFETY: TODO
            self.shared_data.slab.allocated_count
        }
    }

    fn inc_alloc_count(&mut self) -> usize {
        unsafe {
            // SAFETY: TODO
            self.shared_data.slab.allocated_count += 1;

            self.shared_data.slab.allocated_count
        }
    }

    fn dec_alloc_count(&mut self) -> usize {
        unsafe {
            // SAFETY: TODO
            self.shared_data.slab.allocated_count -= 1;

            self.shared_data.slab.allocated_count
        }
    }

    unsafe fn from_allocated(ptr: NonNull<u8>) -> &'static mut Self {
        unsafe {
            // SAFETY: The caller ensures that `ptr` is valid.
            let paddr = ArchPhysAccess::from_ptr(ptr);
            let pfn = PFN::from(paddr);

            ZONE.get_page(pfn)
                .expect("Page outside of the global zone")
                .as_mut()
        }
    }
}

impl PerCpuPage for RawPage {
    fn set_local(&mut self, val: bool) {
        if val {
            self.flags.set(PageFlags::LOCAL)
        } else {
            self.flags.clear(PageFlags::LOCAL)
        }
    }
}

pub struct RawPageList(List);

unsafe impl Send for RawPageList {}

impl FolioList for RawPageList {
    type Folio = RawPage;

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn peek_head(&mut self) -> Option<&mut Self::Folio> {
        unsafe {
            let link = self.0.head()?;
            let mut raw_page_ptr = container_of!(link, RawPage, link);

            Some(raw_page_ptr.as_mut())
        }
    }

    fn pop_head(&mut self) -> Option<&'static mut Self::Folio> {
        unsafe {
            let link = self.0.pop()?;
            let mut raw_page_ptr = container_of!(link, RawPage, link);

            Some(raw_page_ptr.as_mut())
        }
    }

    fn push_tail(&mut self, page: &'static mut Self::Folio) {
        self.0.insert(&mut page.link);
    }

    fn remove(&mut self, page: &mut Self::Folio) {
        self.0.remove(&mut page.link)
    }
}

impl FolioListSized for RawPageList {
    const NEW: Self = RawPageList(List::new());
}

unsafe impl SlabPageAlloc for GlobalPageAlloc {
    type Page = RawPage;
    type PageList = RawPageList;

    fn alloc_slab_page(&self) -> &'static mut RawPage {
        let raw_page = self.alloc_raw_order(0).expect("Out of memory");
        raw_page.flags.set(PageFlags::SLAB);
        raw_page.shared_data.slab = SlabPageData::new();

        raw_page
    }
}
