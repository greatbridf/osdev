use super::address::{PAddr, PFN};
use crate::intrusive_list::Link;
use crate::{container_of, prelude::*};
use bitflags::bitflags;
use core::sync::atomic::Ordering;
use core::{ptr::NonNull, sync::atomic::AtomicU32};

const MAX_PAGE_ORDER: u32 = 10;
const PAGE_ALLOC_COSTLY_ORDER: u32 = 3;
const BATCH_SIZE: u32 = 64;
const PAGE_ARRAY: *mut Page = 0xffffff8040000000 as *mut Page;

pub(super) type PagePtr = Ptr<Page>;

#[repr(transparent)]
pub struct Ptr<T>(Option<NonNull<T>>);

impl<T> Clone for Ptr<T> {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}

impl<T> Copy for Ptr<T> {}

impl<T> Ptr<T> {
    pub const fn new(ptr: Option<NonNull<T>>) -> Self {
        Self(ptr)
    }

    pub fn from_raw(ptr: *mut T) -> Self {
        Self::new(NonNull::new(ptr))
    }

    pub fn null() -> Self {
        Self::new(None)
    }

    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }

    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }

    pub fn as_ptr(&self) -> *mut T {
        self.0.unwrap().as_ptr()
    }

    pub fn as_ref<'a>(&self) -> &'a T {
        unsafe { &*self.as_ptr() }
    }

    pub fn as_mut<'a>(&self) -> &'a mut T {
        unsafe { &mut *self.as_ptr() }
    }
}

impl PagePtr {
    pub unsafe fn increase_refcount(&self) -> u32 {
        self.as_mut().increase_refcount()
    }

    pub unsafe fn decrease_refcount(&self) -> u32 {
        self.as_mut().decrease_refcount()
    }

    pub unsafe fn load_refcount(&self) -> u32 {
        self.as_ref().refcount.load(Ordering::Acquire)
    }

    fn get_order(&self) -> u32 {
        self.as_ref().order
    }

    pub fn is_valid(&self, order: u32) -> bool {
        self.is_some() && self.get_order() == order
    }

    fn offset(&self, count: usize) -> Self {
        match self.0 {
            Some(non_null_ptr) => {
                let new_raw_ptr = unsafe { non_null_ptr.as_ptr().add(count) };
                Self::from_raw(new_raw_ptr)
            }
            None => Self::null(),
        }
    }
}

impl Into<PFN> for PagePtr {
    fn into(self) -> PFN {
        unsafe { PFN::from(self.as_ptr().offset_from(PAGE_ARRAY) as usize) }
    }
}

impl From<PFN> for PagePtr {
    fn from(pfn: PFN) -> Self {
        unsafe { Self::from_raw(PAGE_ARRAY.add(pfn.0)) }
    }
}

bitflags! {
    // TODO: Use atomic
    struct PageFlags: usize {
        const PRESENT = 1 << 0;
        const LOCKED  = 1 << 1;
        const BUDDY   = 1 << 2;
        const SLAB    = 1 << 3;
        const DIRTY   = 1 << 4;
        const FREE    = 1 << 5;
        const LOCAL   = 1 << 6;
    }
}

pub(super) struct Page {
    // Now only used for free page links in the buddy system.
    // Can be used for LRU page swap in the future.
    link: Link,
    flags: PageFlags, // TODO: This should be atomic.
    /// # Safety
    /// This field is only used in buddy system, which is protected by the global lock.
    order: u32,
    refcount: AtomicU32,
}

struct FreeArea {
    free_list: Link,
    count: usize,
}

/// Safety: `Zone` is `Send` because the `PAGE_ARRAY` is shared between cores.
unsafe impl Send for Zone {}
// /// Safety: TODO
// unsafe impl Sync for Zone {}

struct Zone {
    free_areas: [FreeArea; MAX_PAGE_ORDER as usize + 1],
}

struct PerCpuPages {
    batch: u32,
    _high: u32, // TODO: use in future
    free_areas: [FreeArea; PAGE_ALLOC_COSTLY_ORDER as usize + 1],
}

impl PerCpuPages {
    const fn new() -> Self {
        Self {
            batch: BATCH_SIZE,
            _high: 0,
            free_areas: [const { FreeArea::new() }; PAGE_ALLOC_COSTLY_ORDER as usize + 1],
        }
    }

    fn get_free_pages(&mut self, order: u32) -> PagePtr {
        assert!(order <= PAGE_ALLOC_COSTLY_ORDER);

        loop {
            let pages_ptr = self.free_areas[order as usize].get_free_pages();
            if pages_ptr.is_some() {
                return pages_ptr;
            }

            let batch = self.batch >> order;
            ZONE.lock()
                .get_bulk_free_pages(&mut self.free_areas[order as usize], order, batch);
        }
    }

    fn free_pages(&mut self, pages_ptr: PagePtr, order: u32) {
        assert!(order <= PAGE_ALLOC_COSTLY_ORDER);
        assert_eq!(unsafe { pages_ptr.load_refcount() }, 0);
        assert_eq!(pages_ptr.get_order(), order);

        self.free_areas[order as usize].add_pages(pages_ptr);
    }
}

impl Page {
    fn set_flags(&mut self, flags: PageFlags) {
        self.flags.insert(flags);
    }

    fn remove_flags(&mut self, flags: PageFlags) {
        self.flags.remove(flags);
    }

    fn set_order(&mut self, order: u32) {
        self.order = order;
    }

    unsafe fn increase_refcount(&mut self) -> u32 {
        self.refcount.fetch_add(1, Ordering::Relaxed)
    }

    unsafe fn decrease_refcount(&mut self) -> u32 {
        self.refcount.fetch_sub(1, Ordering::AcqRel)
    }

    pub fn is_buddy(&self) -> bool {
        self.flags.contains(PageFlags::BUDDY)
    }

    #[allow(dead_code)]
    pub fn is_slab(&self) -> bool {
        self.flags.contains(PageFlags::SLAB)
    }

    pub fn is_present(&self) -> bool {
        self.flags.contains(PageFlags::PRESENT)
    }

    pub fn is_free(&self) -> bool {
        self.flags.contains(PageFlags::FREE)
    }

    pub fn is_local(&self) -> bool {
        self.flags.contains(PageFlags::LOCAL)
    }
}

impl FreeArea {
    const fn new() -> Self {
        Self {
            free_list: Link::new(),
            count: 0,
        }
    }

    fn get_free_pages(&mut self) -> PagePtr {
        if let Some(pages_link) = self.free_list.next_mut() {
            assert_ne!(self.count, 0);

            let pages_ptr = unsafe { container_of!(pages_link, Page, link) };
            let pages_ptr = Ptr::from_raw(pages_ptr);

            self.count -= 1;
            pages_link.remove();

            pages_ptr
        } else {
            PagePtr::null()
        }
    }

    fn add_pages(&mut self, pages_ptr: PagePtr) {
        self.count += 1;
        pages_ptr.as_mut().set_flags(PageFlags::FREE);
        self.free_list.insert(&mut pages_ptr.as_mut().link)
    }

    fn del_pages(&mut self, pages_ptr: PagePtr) {
        assert!(self.count >= 1 && pages_ptr.as_ref().is_free());
        self.count -= 1;
        pages_ptr.as_mut().remove_flags(PageFlags::FREE);
        pages_ptr.as_mut().link.remove();
    }
}

impl Zone {
    const fn new() -> Self {
        Self {
            free_areas: [const { FreeArea::new() }; MAX_PAGE_ORDER as usize + 1],
        }
    }

    /// Only used for per-cpu pages
    fn get_bulk_free_pages(&mut self, free_area: &mut FreeArea, order: u32, count: u32) -> u32 {
        for i in 0..count {
            let pages_ptr = self.get_free_pages(order);
            if pages_ptr.is_none() {
                return i;
            }

            pages_ptr.as_mut().set_flags(PageFlags::LOCAL);
            free_area.add_pages(pages_ptr);
        }
        count
    }

    fn get_free_pages(&mut self, order: u32) -> PagePtr {
        for current_order in order..=MAX_PAGE_ORDER {
            let pages_ptr = self.free_areas[current_order as usize].get_free_pages();
            if pages_ptr.is_none() {
                continue;
            }

            pages_ptr.as_mut().set_order(order);

            if current_order > order {
                self.expand(pages_ptr, current_order, order);
            }
            assert!(pages_ptr.as_ref().is_present() && pages_ptr.as_ref().is_free());
            return pages_ptr;
        }
        PagePtr::new(None)
    }

    fn expand(&mut self, pages_ptr: PagePtr, order: u32, target_order: u32) {
        assert!(pages_ptr.is_some());
        let mut offset = 1 << order;

        for order in (target_order..order).rev() {
            offset >>= 1;
            let split_pages_ptr = pages_ptr.offset(offset);
            split_pages_ptr.as_mut().set_order(order);
            split_pages_ptr.as_mut().set_flags(PageFlags::BUDDY);
            self.free_areas[order as usize].add_pages(split_pages_ptr);
        }
    }

    fn free_pages(&mut self, mut pages_ptr: PagePtr, order: u32) {
        assert_eq!(unsafe { pages_ptr.load_refcount() }, 0);
        assert_eq!(pages_ptr.get_order(), order);

        let mut pfn: PFN = pages_ptr.into();
        let mut current_order = order;

        while current_order < MAX_PAGE_ORDER {
            let buddy_pfn = pfn.buddy_pfn(current_order);
            let buddy_pages_ptr = PagePtr::from(buddy_pfn);

            if !self.buddy_check(buddy_pages_ptr, current_order) {
                break;
            }

            pages_ptr.as_mut().remove_flags(PageFlags::BUDDY);
            buddy_pages_ptr.as_mut().remove_flags(PageFlags::BUDDY);
            self.free_areas[current_order as usize].del_pages(buddy_pages_ptr);
            pages_ptr = PagePtr::from(pfn.combined_pfn(buddy_pfn));
            pages_ptr.as_mut().set_flags(PageFlags::BUDDY);
            pfn = pfn.combined_pfn(buddy_pfn);
            current_order += 1;
        }

        pages_ptr.as_mut().set_order(current_order);
        self.free_areas[current_order as usize].add_pages(pages_ptr);
    }

    /// This function checks whether a page is free && is the buddy
    /// we can coalesce a page and its buddy if
    /// - the buddy is valid(present) &&
    /// - the buddy is right now in free_areas &&
    /// - a page and its buddy have the same order &&
    /// - a page and its buddy are in the same zone.    // check when smp
    fn buddy_check(&self, pages_ptr: PagePtr, order: u32) -> bool {
        if !pages_ptr.as_ref().is_present() {
            return false;
        }
        if !(pages_ptr.as_ref().is_free()) {
            return false;
        }
        if pages_ptr.as_ref().is_local() {
            return false;
        }
        if pages_ptr.as_ref().order != order {
            return false;
        }

        assert_eq!(unsafe { pages_ptr.load_refcount() }, 0);
        true
    }

    /// Only used on buddy initialization
    fn create_pages(&mut self, start: usize, end: usize) {
        let mut start_pfn = PAddr::from(start).ceil_pfn();
        let end_pfn = PAddr::from(end).floor_pfn();

        while start_pfn < end_pfn {
            let mut order = usize::from(start_pfn).trailing_zeros().min(MAX_PAGE_ORDER);

            while start_pfn + order as usize > end_pfn {
                order -= 1;
            }
            let page_ptr: PagePtr = start_pfn.into();
            page_ptr.as_mut().set_flags(PageFlags::BUDDY);
            self.free_areas[order as usize].add_pages(page_ptr);
            start_pfn = start_pfn + (1 << order) as usize;
        }
    }
}

#[arch::define_percpu]
static PER_CPU_PAGES: PerCpuPages = PerCpuPages::new();

static ZONE: Spin<Zone> = Spin::new(Zone::new());

fn __alloc_pages(order: u32) -> PagePtr {
    let pages_ptr;

    if order <= PAGE_ALLOC_COSTLY_ORDER {
        unsafe {
            pages_ptr = PER_CPU_PAGES.as_mut().get_free_pages(order);
        }
    } else {
        pages_ptr = ZONE.lock().get_free_pages(order);
    }

    unsafe {
        pages_ptr.as_mut().increase_refcount();
    }
    pages_ptr.as_mut().remove_flags(PageFlags::FREE);
    pages_ptr
}

fn __free_pages(pages_ptr: PagePtr, order: u32) {
    if order <= PAGE_ALLOC_COSTLY_ORDER {
        unsafe {
            PER_CPU_PAGES.as_mut().free_pages(pages_ptr, order);
        }
    } else {
        ZONE.lock().free_pages(pages_ptr, order);
    }
}

pub(super) fn alloc_page() -> PagePtr {
    __alloc_pages(0)
}

pub(super) fn alloc_pages(order: u32) -> PagePtr {
    __alloc_pages(order)
}

pub(super) fn early_alloc_pages(order: u32) -> PagePtr {
    let pages_ptr = ZONE.lock().get_free_pages(order);
    unsafe {
        pages_ptr.as_mut().increase_refcount();
    }
    pages_ptr.as_mut().remove_flags(PageFlags::FREE);
    pages_ptr
}

pub(super) fn free_pages(page_ptr: PagePtr, order: u32) {
    __free_pages(page_ptr, order)
}

#[no_mangle]
pub extern "C" fn mark_present(start: usize, end: usize) {
    let mut start_pfn = PAddr::from(start).ceil_pfn();
    let end_pfn = PAddr::from(end).floor_pfn();
    while start_pfn < end_pfn {
        PagePtr::from(start_pfn)
            .as_mut()
            .set_flags(PageFlags::PRESENT);
        start_pfn = start_pfn + 1;
    }
}

#[no_mangle]
pub extern "C" fn create_pages(start: usize, end: usize) {
    ZONE.lock().create_pages(start, end);
}

#[no_mangle]
pub extern "C" fn page_to_pfn(page: *const Page) -> usize {
    unsafe { page.offset_from(PAGE_ARRAY) as usize }
}

#[no_mangle]
pub extern "C" fn c_alloc_page() -> *const Page {
    alloc_page().as_ptr() as *const Page
}

#[no_mangle]
pub extern "C" fn c_alloc_pages(order: u32) -> *const Page {
    alloc_pages(order).as_ptr() as *const Page
}

#[no_mangle]
pub extern "C" fn c_alloc_page_table() -> usize {
    let pfn: PFN = alloc_page().into();
    let paddr: usize = usize::from(pfn) << 12;
    unsafe {
        core::ptr::write_bytes(paddr as *mut u8, 0, 4096);
    }
    paddr
}
