use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use buddy_allocator::BuddyRawPage;
use eonix_hal::mm::ArchPhysAccess;
use eonix_mm::address::{PAddr, PhysAccess as _};
use eonix_mm::paging::{PageAlloc, RawPage as RawPageTrait, PFN};
use intrusive_list::{container_of, Link, List};
use slab_allocator::{SlabPage, SlabPageAlloc, SlabPageList, SlabSlot};

use super::GlobalPageAlloc;
use crate::kernel::mem::page_cache::PageCacheRawPage;
use crate::kernel::mem::PhysAccess;

const PAGE_ARRAY: NonNull<RawPage> =
    unsafe { NonNull::new_unchecked(0xffffff8040000000 as *mut _) };

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

#[derive(Clone, Copy)]
struct PageCacheData {
    valid_size: usize,
}

#[repr(C)]
union PageData {
    slab: SlabPageData,
    page_cache: PageCacheData,
}

pub struct RawPage {
    /// This can be used for LRU page swap in the future.
    ///
    /// Now only used for free page links in the buddy system.
    link: Link,
    /// # Safety
    /// This field is only used in buddy system and is protected by the global lock.
    order: u32,
    flags: PageFlags,
    refcount: AtomicUsize,

    shared_data: PageData,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RawPagePtr(NonNull<RawPage>);

impl PageFlags {
    pub const PRESENT: u32 = 1 << 0;
    pub const LOCKED: u32 = 1 << 1;
    pub const BUDDY: u32 = 1 << 2;
    pub const SLAB: u32 = 1 << 3;
    pub const DIRTY: u32 = 1 << 4;
    pub const FREE: u32 = 1 << 5;
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

impl RawPagePtr {
    pub const fn from_ref(raw_page_ref: &RawPage) -> Self {
        Self::new(unsafe {
            // SAFETY: Rust references always points to non-null addresses.
            NonNull::new_unchecked(&raw const *raw_page_ref as *mut _)
        })
    }

    pub const fn new(ptr: NonNull<RawPage>) -> Self {
        Self(ptr)
    }

    /// Get a raw pointer to the underlying `RawPage` struct.
    ///
    /// # Safety
    /// Doing arithmetic on the pointer returned will cause immediate undefined behavior.
    pub const unsafe fn as_ptr(self) -> *mut RawPage {
        self.0.as_ptr()
    }

    pub const fn as_ref<'a>(self) -> &'a RawPage {
        unsafe { &*self.as_ptr() }
    }

    pub const fn as_mut<'a>(self) -> &'a mut RawPage {
        unsafe { &mut *self.as_ptr() }
    }

    pub const fn order(&self) -> u32 {
        self.as_ref().order
    }

    pub const fn flags(&self) -> &PageFlags {
        &self.as_ref().flags
    }

    pub const fn refcount(&self) -> &AtomicUsize {
        &self.as_ref().refcount
    }

    // return the ptr point to the actually raw page
    pub fn real_ptr<T>(&self) -> NonNull<T> {
        let pfn = unsafe { PFN::from(RawPagePtr(NonNull::new_unchecked(self.as_ptr()))) };
        unsafe { PAddr::from(pfn).as_ptr::<T>() }
    }
}

impl From<RawPagePtr> for PFN {
    fn from(value: RawPagePtr) -> Self {
        let idx = unsafe { value.as_ptr().offset_from(PAGE_ARRAY.as_ptr()) as usize };
        Self::from(idx)
    }
}

impl From<PFN> for RawPagePtr {
    fn from(pfn: PFN) -> Self {
        let raw_page_ptr = unsafe { PAGE_ARRAY.add(usize::from(pfn)) };
        Self::new(raw_page_ptr)
    }
}

impl RawPageTrait for RawPagePtr {
    fn order(&self) -> u32 {
        self.order()
    }

    fn refcount(&self) -> &AtomicUsize {
        self.refcount()
    }

    fn is_present(&self) -> bool {
        self.flags().has(PageFlags::PRESENT)
    }
}

impl BuddyRawPage for RawPagePtr {
    unsafe fn from_link(link: &mut Link) -> Self {
        let raw_page_ptr = container_of!(link, RawPage, link);
        Self(raw_page_ptr)
    }

    fn set_order(&self, order: u32) {
        self.as_mut().order = order;
    }

    unsafe fn get_link(&self) -> &mut Link {
        &mut self.as_mut().link
    }

    fn is_buddy(&self) -> bool {
        self.flags().has(PageFlags::BUDDY)
    }

    fn is_free(&self) -> bool {
        self.flags().has(PageFlags::FREE)
    }

    fn set_buddy(&self) {
        self.flags().set(PageFlags::BUDDY);
    }

    fn set_free(&self) {
        self.flags().set(PageFlags::FREE);
    }

    fn clear_buddy(&self) {
        self.flags().clear(PageFlags::BUDDY);
    }

    fn clear_free(&self) {
        self.flags().clear(PageFlags::FREE);
    }
}

impl SlabPage for RawPage {
    fn get_data_ptr(&self) -> NonNull<[u8]> {
        let raw_page_ptr = RawPagePtr::from_ref(self);
        let paddr_start = PAddr::from(PFN::from(raw_page_ptr));
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

            RawPagePtr::from(pfn).as_mut()
        }
    }
}

impl PageCacheRawPage for RawPagePtr {
    fn valid_size(&self) -> &mut usize {
        unsafe {
            // SAFETY: The caller ensures that the page is in some page cache.
            &mut self.as_mut().shared_data.page_cache.valid_size
        }
    }

    fn is_dirty(&self) -> bool {
        self.flags().has(PageFlags::DIRTY)
    }

    fn clear_dirty(&self) {
        self.flags().clear(PageFlags::DIRTY);
    }

    fn set_dirty(&self) {
        self.flags().set(PageFlags::DIRTY);
    }

    fn cache_init(&self) {
        self.as_mut().shared_data.page_cache = PageCacheData { valid_size: 0 };
    }
}

pub struct RawSlabPageList(List);

impl SlabPageList for RawSlabPageList {
    type Page = RawPage;

    fn new() -> Self {
        Self(List::new())
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn peek_head(&mut self) -> Option<&mut Self::Page> {
        unsafe {
            let link = self.0.head()?;
            let mut raw_page_ptr = container_of!(link, RawPage, link);

            Some(raw_page_ptr.as_mut())
        }
    }

    fn pop_head(&mut self) -> Option<&'static mut Self::Page> {
        unsafe {
            let link = self.0.pop()?;
            let mut raw_page_ptr = container_of!(link, RawPage, link);

            Some(raw_page_ptr.as_mut())
        }
    }

    fn push_tail(&mut self, page: &'static mut Self::Page) {
        self.0.insert(&mut page.link);
    }

    fn remove(&mut self, page: &mut Self::Page) {
        self.0.remove(&mut page.link)
    }
}

impl SlabPageAlloc for GlobalPageAlloc {
    type Page = RawPage;
    type PageList = RawSlabPageList;

    unsafe fn alloc_uninit(&self) -> &'static mut RawPage {
        let raw_page = self.alloc().expect("Out of memory").as_mut();
        raw_page.flags.set(PageFlags::SLAB);
        raw_page.shared_data.slab = SlabPageData::new();

        raw_page
    }
}
