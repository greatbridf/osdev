use buddy_allocator::BuddyRawPage;
use core::{
    ptr::NonNull,
    sync::atomic::{AtomicU32, AtomicUsize, Ordering},
};
use eonix_mm::paging::{RawPage as RawPageTrait, PFN};
use intrusive_list::{container_of, Link};

const PAGE_ARRAY: NonNull<RawPage> =
    unsafe { NonNull::new_unchecked(0xffffff8040000000 as *mut _) };

pub struct PageFlags(AtomicU32);

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
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RawPagePtr(NonNull<RawPage>);

impl PageFlags {
    pub const PRESENT: u32 = 1 << 0;
    // pub const LOCKED: u32 = 1 << 1;
    pub const BUDDY: u32 = 1 << 2;
    // pub const SLAB: u32 = 1 << 3;
    // pub const DIRTY: u32 = 1 << 4;
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
}

impl RawPagePtr {
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
