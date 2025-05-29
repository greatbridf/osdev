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

pub struct SlabPageInner {
    pub object_size: u32,
    pub allocated_count: u32,
    pub free_next: Option<NonNull<usize>>,
}

pub struct BuddyPageInner {}

pub enum PageType {
    Buddy(BuddyPageInner),
    Slab(SlabPageInner),
}

impl PageType {
    // slab
    pub fn new_slab(&mut self, object_size: u32) {
        *self = PageType::Slab(SlabPageInner {
            object_size,
            allocated_count: 0,
            free_next: None,
        })
    }

    pub fn object_size(&self) -> u32 {
        assert!(matches!(self, PageType::Slab(_)));

        match self {
            PageType::Slab(inner) => inner.object_size,
            _ => unsafe { core::hint::unreachable_unchecked() },
        }
    }

    pub fn allocated_count(&self) -> u32 {
        assert!(matches!(self, PageType::Slab(_)));

        match self {
            PageType::Slab(inner) => inner.allocated_count,
            _ => unsafe { core::hint::unreachable_unchecked() },
        }
    }

    pub fn allocated_count_add(&mut self, val: u32) {
        assert!(matches!(self, PageType::Slab(_)));

        match self {
            PageType::Slab(inner) => inner.allocated_count += val,
            _ => unsafe { core::hint::unreachable_unchecked() },
        }
    }

    pub fn allocated_count_sub(&mut self, val: u32) {
        assert!(matches!(self, PageType::Slab(_)));

        match self {
            PageType::Slab(inner) => inner.allocated_count -= val,
            _ => unsafe { core::hint::unreachable_unchecked() },
        }
    }

    pub fn free_next(&self) -> Option<NonNull<usize>> {
        assert!(matches!(self, PageType::Slab(_)));

        match self {
            PageType::Slab(inner) => inner.free_next,
            _ => unsafe { core::hint::unreachable_unchecked() },
        }
    }

    pub fn set_free_next(&mut self, free_next: Option<NonNull<usize>>) {
        assert!(matches!(self, PageType::Slab(_)));

        match self {
            PageType::Slab(inner) => inner.free_next = free_next,
            _ => unsafe { core::hint::unreachable_unchecked() },
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

    pub type_: PageType,
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

    fn is_emtpy(&self) -> bool {
        self.as_ref().type_.allocated_count() == 0
    }

    fn is_full(&self) -> bool {
        self.as_ref().type_.free_next().is_none()
    }

    fn alloc_slot(&self) -> *mut u8 {
        let ptr = self.as_ref().type_.free_next();

        match ptr {
            Some(ptr) => {
                let next_free = unsafe { ptr.read() as *mut usize };
                self.as_mut().type_.set_free_next(NonNull::new(next_free));
                self.as_mut().type_.allocated_count_add(1);
                return ptr.as_ptr() as *mut u8;
            }
            None => unreachable!(),
        }
    }

    fn in_which(ptr: *mut u8) -> RawPagePtr {
        let vaddr = VAddr::from(ptr as usize & !(PAGE_SIZE - 1));

        unsafe { vaddr.as_raw_page() }
    }

    fn dealloc_slot(&self, ptr: *mut u8) {
        let ptr = ptr as *mut usize;

        if let Some(last_free) = self.as_ref().type_.free_next() {
            unsafe { *ptr = last_free.as_ptr() as usize }
        } else {
            unsafe { *ptr = 0 }
        }

        self.as_mut().type_.allocated_count_sub(1);
        self.as_mut().type_.set_free_next(NonNull::new(ptr));
    }

    fn slab_init(&self, object_size: u32) {
        assert!(object_size >= core::mem::size_of::<usize>() as u32);

        self.as_mut().type_.new_slab(object_size);

        let mut slot_count = PAGE_SIZE / object_size as usize;
        let mut ptr = self.real_ptr::<usize>().as_ptr();
        self.as_mut().type_.set_free_next(NonNull::new(ptr));

        // SAFETY: carefully ptr operate
        unsafe {
            loop {
                if slot_count == 1 {
                    *ptr = 0;
                    break;
                }

                let next_ptr = ptr.byte_add(object_size as usize);
                *ptr = next_ptr as usize;
                ptr = next_ptr;
                slot_count -= 1;
            }
        }
    }
}
