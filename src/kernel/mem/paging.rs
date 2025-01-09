use super::address::PFN;
use super::page_alloc::{alloc_page, alloc_pages, early_alloc_pages, free_pages, PagePtr};
use super::phys::PhysPtr;
use crate::io::{Buffer, FillResult};
use crate::kernel::mem::phys;
use core::fmt;

pub struct Page {
    page_ptr: PagePtr,
    order: u32,
}

#[allow(dead_code)]
impl Page {
    pub fn alloc_one() -> Self {
        let page_ptr = alloc_page();
        Self { page_ptr, order: 0 }
    }

    pub fn alloc_many(order: u32) -> Self {
        let page_ptr = alloc_pages(order);
        Self { page_ptr, order }
    }

    /// Allocate a contiguous block of pages that can contain at least `count` pages.
    pub fn alloc_ceil(count: usize) -> Self {
        assert_ne!(count, 0);
        let order = count.next_power_of_two().trailing_zeros();
        Self::alloc_many(order)
    }

    pub fn early_alloc_ceil(count: usize) -> Self {
        assert_ne!(count, 0);
        let order = count.next_power_of_two().trailing_zeros();
        let page_ptr = early_alloc_pages(order);
        Self { page_ptr, order }
    }

    /// Get `Page` from `pfn`, acquiring the ownership of the page. `refcount` is not increased.
    ///
    /// # Safety
    /// Caller must ensure that the pfn is no longer referenced by any other code.
    pub unsafe fn take_pfn(pfn: usize, order: u32) -> Self {
        let page_ptr: PagePtr = PFN::from(pfn >> 12).into();

        // Only buddy pages can be used here.
        // Also, check if the order is correct.
        assert!(page_ptr.as_ref().is_buddy() && page_ptr.is_valid(order));

        Self { page_ptr, order }
    }

    /// Get `Page` from `pfn` and increase the reference count.
    ///
    /// # Safety
    /// Caller must ensure that `pfn` refers to a valid physical frame number with `refcount` > 0.
    pub unsafe fn from_pfn(pfn: usize, order: u32) -> Self {
        // SAFETY: `pfn` is a valid physical frame number with refcount > 0.
        Self::increase_refcount(pfn);

        // SAFETY: `pfn` has an increased refcount.
        unsafe { Self::take_pfn(pfn, order) }
    }

    /// Consumes the `Page` and returns the physical frame number without dropping the reference
    /// count the page holds.
    pub fn into_pfn(self) -> usize {
        let pfn: PFN = self.page_ptr.into();
        core::mem::forget(self);
        usize::from(pfn) << 12
    }

    pub fn len(&self) -> usize {
        1 << (self.order + 12)
    }

    pub fn as_phys(&self) -> usize {
        let pfn: PFN = self.page_ptr.into();
        usize::from(pfn) << 12
    }

    pub fn as_cached(&self) -> phys::CachedPP {
        phys::CachedPP::new(self.as_phys())
    }

    pub fn as_nocache(&self) -> phys::NoCachePP {
        phys::NoCachePP::new(self.as_phys())
    }

    pub fn as_slice<'r, 'lt>(&'r self) -> &'lt [u8] {
        self.as_cached().as_slice(self.len())
    }

    pub fn as_mut_slice<'r, 'lt>(&'r self) -> &'lt mut [u8] {
        self.as_cached().as_mut_slice(self.len())
    }

    pub fn zero(&self) {
        self.as_mut_slice().fill(0);
    }

    /// # Safety
    /// Caller must ensure that the page is properly freed.
    pub unsafe fn increase_refcount(pfn: usize) {
        let page_ptr: PagePtr = PFN::from(pfn >> 12).into();
        page_ptr.increase_refcount();
    }

    pub unsafe fn load_refcount(&self) -> usize {
        self.page_ptr.load_refcount() as usize
    }
}

impl Clone for Page {
    fn clone(&self) -> Self {
        unsafe { self.page_ptr.increase_refcount() };

        Self {
            page_ptr: self.page_ptr,
            order: self.order,
        }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        match unsafe { self.page_ptr.decrease_refcount() } {
            0 => panic!("In-use page refcount is 0"),
            1 => free_pages(self.page_ptr, self.order),
            _ => {}
        }
    }
}

impl PartialEq for Page {
    fn eq(&self, other: &Self) -> bool {
        // assert!(self.page_ptr != other.page_ptr || self.order == other.order);

        self.page_ptr.as_ptr() == other.page_ptr.as_ptr()
    }
}

unsafe impl Sync for Page {}
unsafe impl Send for Page {}

impl fmt::Debug for Page {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let pfn = self.as_phys();
        write!(f, "Page({:#x}, order={})", pfn, self.order)
    }
}

pub struct PageBuffer {
    page: Page,
    offset: usize,
}

#[allow(dead_code)]
impl PageBuffer {
    pub fn new(page: Page) -> Self {
        Self { page, offset: 0 }
    }

    pub fn len(&self) -> usize {
        self.offset
    }

    pub fn remaining(&self) -> usize {
        self.page.len() - self.offset
    }

    pub fn as_slice(&self) -> &[u8] {
        self.page.as_slice()
    }

    fn available_as_slice(&self) -> &mut [u8] {
        &mut self.page.as_mut_slice()[self.offset..]
    }
}

impl Buffer for PageBuffer {
    fn total(&self) -> usize {
        self.page.len()
    }

    fn wrote(&self) -> usize {
        self.len()
    }

    fn fill(&mut self, data: &[u8]) -> crate::KResult<crate::io::FillResult> {
        if self.remaining() == 0 {
            return Ok(FillResult::Full);
        }

        let len = core::cmp::min(data.len(), self.remaining());
        self.available_as_slice()[..len].copy_from_slice(&data[..len]);
        self.offset += len;

        if len < data.len() {
            Ok(FillResult::Partial(len))
        } else {
            Ok(FillResult::Done(len))
        }
    }
}
