use crate::bindings::root::EFAULT;
use crate::kernel::mem::phys;
use core::fmt;

use super::phys::PhysPtr;

pub struct Page {
    page_ptr: *mut crate::bindings::root::kernel::mem::paging::page,
    order: u32,
}

impl Page {
    pub fn alloc_one() -> Self {
        use crate::bindings::root::kernel::mem::paging::alloc_page;
        let page_ptr = unsafe { alloc_page() };

        Self { page_ptr, order: 0 }
    }

    pub fn alloc_many(order: u32) -> Self {
        use crate::bindings::root::kernel::mem::paging::alloc_pages;
        let page_ptr = unsafe { alloc_pages(order) };

        Self { page_ptr, order }
    }

    pub fn len(&self) -> usize {
        1 << (self.order + 12)
    }

    pub fn as_phys(&self) -> usize {
        use crate::bindings::root::kernel::mem::paging::page_to_pfn;

        unsafe { page_to_pfn(self.page_ptr) }
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
            core::ptr::write_bytes(
                self.as_cached().as_ptr::<u8>(),
                0,
                self.len(),
            );
        }
    }
}

impl Clone for Page {
    fn clone(&self) -> Self {
        unsafe {
            crate::bindings::root::kernel::mem::paging::increase_refcount(
                self.page_ptr,
            );
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
            crate::bindings::root::kernel::mem::paging::free_pages(
                self.page_ptr,
                self.order,
            );
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
        unsafe {
            core::slice::from_raw_parts(
                self.page.as_cached().as_ptr::<u8>(),
                self.offset,
            )
        }
    }

    pub fn as_mut_slice(&self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.page.as_cached().as_ptr::<u8>(),
                self.offset,
            )
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
        core::ptr::copy_nonoverlapping(
            src.as_ptr(),
            dst.as_cached().as_ptr(),
            src.len(),
        );
    }

    Ok(())
}
