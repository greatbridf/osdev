use super::{access::AsMemoryBlock, page_alloc::GlobalPageAlloc, MemoryBlock, PhysAccess};
use crate::io::{Buffer, FillResult};
use eonix_mm::paging::{NoAlloc, Page as GenericPage, PageAlloc};

pub type PageUnmanaged = GenericPage<NoAlloc>;
pub type Page = GenericPage<GlobalPageAlloc>;

/// A buffer that wraps a page and provides a `Buffer` interface.
pub struct PageBuffer {
    page: Page,
    offset: usize,
}

pub trait AllocZeroed {
    fn zeroed() -> Self;
}

impl<A: PageAlloc> AsMemoryBlock for GenericPage<A> {
    fn as_memblk(&self) -> MemoryBlock {
        unsafe {
            // SAFETY: `self.start()` points to valid memory of length `self.len()`.
            MemoryBlock::new(self.start().as_ptr::<()>().addr(), self.len())
        }
    }
}

impl PageBuffer {
    pub fn new() -> Self {
        Self {
            page: Page::alloc(),
            offset: 0,
        }
    }

    pub fn all(&self) -> &[u8] {
        unsafe {
            // SAFETY: The page is exclusivly owned by us.
            self.page.as_memblk().as_bytes()
        }
    }

    pub fn data(&self) -> &[u8] {
        &self.all()[..self.offset]
    }

    pub fn available_mut(&mut self) -> &mut [u8] {
        unsafe {
            // SAFETY: The page is exclusivly owned by us.
            &mut self.page.as_memblk().as_bytes_mut()[self.offset..]
        }
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
        unsafe {
            // SAFETY: The page is exclusivly owned by us.
            page.as_memblk().as_bytes_mut().fill(0);
        }
        page
    }
}
