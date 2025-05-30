use buddy_allocator::BuddyRawPage;
use core::{
    ptr::NonNull,
    sync::atomic::{AtomicU32, AtomicUsize, Ordering},
};
use eonix_mm::{
    address::{PAddr, VAddr},
    paging::{RawPage as RawPageTrait, PAGE_SIZE, PFN},
};
use intrusive_list::{container_of, Link};
use slab_allocator::SlabRawPage;

use crate::kernel::mem::access::RawPageAccess;
use crate::kernel::mem::PhysAccess;

const PAGE_ARRAY: NonNull<RawPage> =
    unsafe { NonNull::new_unchecked(0xffffff8040000000 as *mut _) };

pub struct PageFlags(AtomicU32);

struct SlabPageInner {
    allocated_count: u32,
    free_next: Option<NonNull<usize>>,
}

impl SlabPageInner {
    fn new(free_next: Option<NonNull<usize>>) -> Self {
        Self {
            allocated_count: 0,
            free_next,
        }
    }
}

pub struct BuddyPageInner {}

pub enum PageType {
    Buddy(BuddyPageInner),
    Slab(SlabPageInner),
}

impl PageType {
    pub fn slab_data(&mut self) -> &mut SlabPageInner {
        if let PageType::Slab(slab_data) = self {
            return slab_data;
        } else {
            unreachable!()
        }
    }
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

    pub shared_data: PageType,
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

impl SlabRawPage for RawPagePtr {
    unsafe fn from_link(link: &mut Link) -> Self {
        let raw_page_ptr = container_of!(link, RawPage, link);
        Self(raw_page_ptr)
    }

    unsafe fn get_link(&self) -> &mut Link {
        &mut self.as_mut().link
    }

    fn in_which(ptr: *mut u8) -> RawPagePtr {
        let vaddr = VAddr::from(ptr as usize & !(PAGE_SIZE - 1));
        unsafe { vaddr.as_raw_page() }
    }

    fn allocated_count(&self) -> &mut u32 {
        &mut self.as_mut().shared_data.slab_data().allocated_count
    }

    fn next_free(&self) -> &mut Option<NonNull<usize>> {
        &mut self.as_mut().shared_data.slab_data().free_next
    }

    fn real_page_ptr(&self) -> *mut u8 {
        self.real_ptr().as_ptr()
    }

    fn slab_init(&self, first_free: Option<NonNull<usize>>) {
        self.as_mut().shared_data = PageType::Slab(SlabPageInner::new(first_free));
    }
}
