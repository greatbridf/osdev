use core::fmt::Debug;

use eonix_mm::paging::{PAGE_SIZE, PAGE_SIZE_BITS};

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PageOffset(usize);

impl PageOffset {
    pub const fn from_byte_floor(offset: usize) -> Self {
        Self(offset >> PAGE_SIZE_BITS)
    }

    pub const fn from_byte_ceil(offset: usize) -> Self {
        Self((offset + PAGE_SIZE - 1) >> PAGE_SIZE_BITS)
    }

    pub fn iter_till(
        self, end: PageOffset,
    ) -> impl Iterator<Item = PageOffset> {
        (self.0..end.0).map(PageOffset)
    }

    pub fn page_count(self) -> usize {
        self.0
    }

    pub fn byte_count(self) -> usize {
        self.page_count() * PAGE_SIZE
    }
}

impl Debug for PageOffset {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PageOffset({:#x})", self.0)
    }
}
