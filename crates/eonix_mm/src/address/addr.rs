use crate::paging::PAGE_SIZE;
use core::ops::{Add, Sub};

#[doc(notable_trait)]
pub trait Addr:
    Sized
    + Copy
    + Clone
    + Ord
    + PartialOrd
    + Eq
    + PartialEq
    + Sub<Output = usize>
    + Sub<usize, Output = Self>
    + Add<usize, Output = Self>
    + From<usize>
{
    fn addr(self) -> usize;
}

#[doc(notable_trait)]
pub trait AddrOps: Sized {
    fn offset_in(self, size: usize) -> usize;

    fn is_aligned_to(self, size: usize) -> bool;

    /// Aligns the address to the nearest lower multiple of `size`.
    fn floor_to(self, size: usize) -> Self;

    /// Aligns the address to the nearest lower multiple of `size`.
    fn ceil_to(self, size: usize) -> Self;

    fn page_offset(self) -> usize {
        self.offset_in(PAGE_SIZE)
    }

    fn is_page_aligned(self) -> bool {
        self.is_aligned_to(PAGE_SIZE)
    }

    fn floor(self) -> Self {
        self.floor_to(PAGE_SIZE)
    }

    fn ceil(self) -> Self {
        self.ceil_to(PAGE_SIZE)
    }
}

impl<A: Addr> AddrOps for A {
    fn offset_in(self, size: usize) -> usize {
        self.addr() % size
    }

    fn is_aligned_to(self, size: usize) -> bool {
        self.offset_in(size) == 0
    }

    fn floor_to(self, size: usize) -> Self {
        Self::from(self.addr() / size * size)
    }

    fn ceil_to(self, size: usize) -> Self {
        Self::from(self.addr().div_ceil(size) * size)
    }
}
