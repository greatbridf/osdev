use crate::bindings::root::kernel::mem::paging::{
    alloc_page as c_alloc_page, alloc_pages as c_alloc_pages, free_pages as c_free_pages,
    increase_refcount as c_increase_refcount, page as c_page, page_to_pfn as c_page_to_pfn,
    pfn_to_page as c_pfn_to_page, PAGE_BUDDY,
};
use crate::bindings::root::EFAULT;
use crate::io::{Buffer, FillResult};
use crate::kernel::mem::phys;
use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};

use super::phys::PhysPtr;

fn msb(x: u64) -> u64 {
    // What the ****.
    let mut x = x;
    x |= x >> 1;
    x |= x >> 2;
    x |= x >> 4;
    x |= x >> 8;
    x |= x >> 16;
    x |= x >> 32;
    x -= x >> 1;
    x
}

fn msb_position(x: u64) -> Option<u32> {
    if x == 0 {
        return None;
    }

    let mut pos = 0;
    let mut x = x;
    if x >= 1 << 32 {
        x >>= 32;
        pos += 32;
    }
    if x >= 1 << 16 {
        x >>= 16;
        pos += 16;
    }
    if x >= 1 << 8 {
        x >>= 8;
        pos += 8;
    }
    if x >= 1 << 4 {
        x >>= 4;
        pos += 4;
    }
    if x >= 1 << 2 {
        x >>= 2;
        pos += 2;
    }
    if x >= 1 {
        pos += 1;
    }

    Some(pos)
}

pub struct Page {
    page_ptr: *mut c_page,
    order: u32,
}

#[allow(dead_code)]
impl Page {
    pub fn alloc_one() -> Self {
        let page_ptr = unsafe { c_alloc_page() };

        Self { page_ptr, order: 0 }
    }

    pub fn alloc_many(order: u32) -> Self {
        let page_ptr = unsafe { c_alloc_pages(order) };

        Self { page_ptr, order }
    }

    /// Allocate a contiguous block of pages that can contain at least `count` pages.
    pub fn alloc_ceil(count: usize) -> Self {
        assert_ne!(count, 0);
        let count_msb = msb(count as u64) as usize;
        let order = msb_position((count + count_msb - 1) as u64)
            .expect("`count` can't be 0, so can't `order`");

        Self::alloc_many(order)
    }

    /// Get `Page` from `pfn`, acquiring the ownership of the page. `refcount` is not increased.
    ///
    /// # Safety
    /// Caller must ensure that the pfn is no longer referenced by any other code.
    pub unsafe fn take_pfn(pfn: usize, order: u32) -> Self {
        let page_ptr = unsafe { c_pfn_to_page(pfn) };

        // Only buddy pages can be used here.
        assert!(unsafe { page_ptr.as_ref().unwrap() }.flags & PAGE_BUDDY != 0);

        // Check if the order is correct.
        assert_eq!(
            unsafe { page_ptr.as_ref().unwrap() }.flags & 0xff,
            order as u64
        );

        Self { page_ptr, order }
    }

    /// Get `Page` from `pfn` and increase the reference count.
    ///
    /// # Safety
    /// Caller must ensure that `pfn` refers to a valid physical frame number with `refcount` > 0.
    pub unsafe fn from_pfn(pfn: usize, order: u32) -> Self {
        // SAFETY: `pfn` is a valid physical frame number with refcount > 0.
        unsafe { Self::increase_refcount(pfn) };

        // SAFETY: `pfn` has an increased refcount.
        unsafe { Self::take_pfn(pfn, order) }
    }

    /// Consumes the `Page` and returns the physical frame number without dropping the reference
    /// count the page holds.
    pub fn into_pfn(self) -> usize {
        let pfn = unsafe { c_page_to_pfn(self.page_ptr) };
        core::mem::forget(self);
        pfn
    }

    pub fn len(&self) -> usize {
        1 << (self.order + 12)
    }

    pub fn as_phys(&self) -> usize {
        unsafe { c_page_to_pfn(self.page_ptr) }
    }

    pub fn as_cached(&self) -> phys::CachedPP {
        phys::CachedPP::new(self.as_phys())
    }

    pub fn as_nocache(&self) -> phys::NoCachePP {
        phys::NoCachePP::new(self.as_phys())
    }

    pub fn zero(&self) {
        use phys::PhysPtr;

        unsafe {
            core::ptr::write_bytes(self.as_cached().as_ptr::<u8>(), 0, self.len());
        }
    }

    /// # Safety
    /// Caller must ensure that the page is properly freed.
    pub unsafe fn increase_refcount(pfn: usize) {
        let page = unsafe { c_pfn_to_page(pfn) };

        unsafe {
            c_increase_refcount(page);
        }
    }

    pub unsafe fn load_refcount(&self) -> usize {
        AtomicU64::from_ptr(&mut (*self.page_ptr).refcount).load(Ordering::Acquire) as usize
    }
}

impl Clone for Page {
    fn clone(&self) -> Self {
        unsafe {
            c_increase_refcount(self.page_ptr);
        }

        Self {
            page_ptr: self.page_ptr,
            order: self.order,
        }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        unsafe {
            c_free_pages(self.page_ptr, self.order);
        }
    }
}

impl PartialEq for Page {
    fn eq(&self, other: &Self) -> bool {
        assert!(self.page_ptr != other.page_ptr || self.order == other.order);

        self.page_ptr == other.page_ptr
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
        unsafe { core::slice::from_raw_parts(self.page.as_cached().as_ptr::<u8>(), self.offset) }
    }

    pub fn as_mut_slice(&self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(self.page.as_cached().as_ptr::<u8>(), self.offset)
        }
    }

    fn available_as_slice(&self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.page.as_cached().as_ptr::<u8>().add(self.offset),
                self.remaining(),
            )
        }
    }

    pub fn consume(&mut self, len: usize) {
        self.offset += len;
    }
}

impl core::fmt::Write for PageBuffer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if s.len() > self.remaining() {
            return Err(core::fmt::Error);
        }

        self.available_as_slice()[..s.len()].copy_from_slice(s.as_bytes());
        self.consume(s.len());

        Ok(())
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
        self.consume(len);

        if len < data.len() {
            Ok(FillResult::Partial(len))
        } else {
            Ok(FillResult::Done(len))
        }
    }
}

/// Copy data from a slice to a `Page`
///
/// DONT USE THIS FUNCTION TO COPY DATA TO MMIO ADDRESSES
///
/// # Returns
///
/// Returns `Err(EFAULT)` if the slice is larger than the page
/// Returns `Ok(())` otherwise
pub fn copy_to_page(src: &[u8], dst: &Page) -> Result<(), u32> {
    use phys::PhysPtr;
    if src.len() > dst.len() {
        return Err(EFAULT);
    }

    unsafe {
        core::ptr::copy_nonoverlapping(src.as_ptr(), dst.as_cached().as_ptr(), src.len());
    }

    Ok(())
}
