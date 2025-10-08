use core::ops::Deref;
use core::ptr::NonNull;

use eonix_mm::paging::Page as GenericPage;

use super::page_alloc::GlobalPageAlloc;
use super::PhysAccess;
use crate::io::{Buffer, FillResult};

pub type Page = GenericPage<GlobalPageAlloc>;

/// A buffer that wraps a page and provides a `Buffer` interface.
pub struct PageBuffer {
    page: PageExcl,
    offset: usize,
}

pub struct PageLocked<'a> {
    page: &'a Page,
}

/// A page that is exclusively owned.
#[repr(transparent)]
pub struct PageExcl(Page);

pub trait AllocZeroed {
    fn zeroed() -> Self;
}

pub trait PageExt {
    fn lock(&self) -> PageLocked;

    /// Get a vmem pointer to the page data as a byte slice.
    fn get_bytes_ptr(&self) -> NonNull<[u8]>;

    /// Get a vmem pointer to the start of the page.
    fn get_ptr(&self) -> NonNull<u8> {
        self.get_bytes_ptr().cast()
    }
}

impl PageBuffer {
    pub fn new() -> Self {
        Self {
            page: PageExcl::alloc(),
            offset: 0,
        }
    }

    pub fn all(&self) -> &[u8] {
        self.page.as_bytes()
    }

    pub fn data(&self) -> &[u8] {
        &self.all()[..self.offset]
    }

    pub fn available_mut(&mut self) -> &mut [u8] {
        &mut self.page.as_bytes_mut()[self.offset..]
    }
}

impl Buffer for PageBuffer {
    fn total(&self) -> usize {
        self.page.len()
    }

    fn wrote(&self) -> usize {
        self.offset
    }

    fn fill(&mut self, data: &[u8]) -> crate::KResult<crate::io::FillResult> {
        let available = self.available_mut();
        if available.len() == 0 {
            return Ok(FillResult::Full);
        }

        let len = core::cmp::min(data.len(), available.len());
        available[..len].copy_from_slice(&data[..len]);
        self.offset += len;

        if len < data.len() {
            Ok(FillResult::Partial(len))
        } else {
            Ok(FillResult::Done(len))
        }
    }
}

impl AllocZeroed for Page {
    fn zeroed() -> Self {
        let page = Self::alloc();

        page.lock().as_bytes_mut().fill(0);

        page
    }
}

impl PageExt for Page {
    fn lock(&self) -> PageLocked {
        // TODO: Actually perform the lock.
        PageLocked { page: self }
    }

    fn get_bytes_ptr(&self) -> NonNull<[u8]> {
        unsafe {
            // SAFETY: `self.start()` can't be null.
            NonNull::slice_from_raw_parts(self.start().as_ptr(), self.len())
        }
    }
}

impl PageLocked<'_> {
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            // SAFETY: `self.start()` points to valid memory of length `self.len()`.
            core::slice::from_raw_parts(self.start().as_ptr().as_ptr(), self.len())
        }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe {
            // SAFETY: `self.start()` points to valid memory of length `self.len()`.
            core::slice::from_raw_parts_mut(self.start().as_ptr().as_ptr(), self.len())
        }
    }
}

impl Deref for PageLocked<'_> {
    type Target = Page;

    fn deref(&self) -> &Self::Target {
        self.page
    }
}

impl PageExcl {
    pub fn alloc() -> Self {
        Self(Page::alloc())
    }

    pub fn alloc_order(order: u32) -> Self {
        Self(Page::alloc_order(order))
    }

    pub fn zeroed() -> Self {
        Self(Page::zeroed())
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            // SAFETY: The page is exclusively owned by us.
            self.get_bytes_ptr().as_ref()
        }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe {
            // SAFETY: The page is exclusively owned by us.
            self.get_bytes_ptr().as_mut()
        }
    }

    pub fn into_page(self) -> Page {
        self.0
    }
}

impl Deref for PageExcl {
    type Target = Page;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
