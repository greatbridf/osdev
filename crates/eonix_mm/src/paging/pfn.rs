use crate::address::{Addr as _, PAddr};
use core::{
    fmt,
    ops::{Add, Sub},
};

use super::PAGE_SIZE_BITS;

#[repr(transparent)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct PFN(usize);

impl From<PFN> for usize {
    fn from(v: PFN) -> Self {
        v.0
    }
}

impl From<usize> for PFN {
    fn from(v: usize) -> Self {
        Self(v)
    }
}

impl Sub for PFN {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl Sub<usize> for PFN {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        PFN(self.0 - rhs)
    }
}

impl Add<usize> for PFN {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        PFN(self.0 + rhs)
    }
}

impl fmt::Debug for PFN {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PFN({:#x})", self.0)
    }
}

impl From<PAddr> for PFN {
    fn from(paddr: PAddr) -> Self {
        Self(paddr.addr() >> PAGE_SIZE_BITS)
    }
}

impl PFN {
    pub const fn from_val(pfn: usize) -> Self {
        Self(pfn)
    }
}
