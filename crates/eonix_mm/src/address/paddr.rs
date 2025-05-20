use super::addr::Addr;
use crate::paging::{PAGE_SIZE_BITS, PFN};
use core::{
    fmt,
    ops::{Add, Sub},
};

#[repr(transparent)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct PAddr(usize);

impl From<usize> for PAddr {
    fn from(v: usize) -> Self {
        Self(v)
    }
}

impl Sub for PAddr {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl Sub<usize> for PAddr {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        PAddr(self.0 - rhs)
    }
}

impl Add<usize> for PAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        PAddr(self.0 + rhs)
    }
}

impl fmt::Debug for PAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PAddr({:#x})", self.0)
    }
}

impl Addr for PAddr {
    fn addr(self) -> usize {
        let Self(addr) = self;
        addr
    }
}

impl From<PFN> for PAddr {
    fn from(value: PFN) -> Self {
        Self(usize::from(value) << PAGE_SIZE_BITS)
    }
}

impl PAddr {
    pub const fn from_val(val: usize) -> Self {
        Self(val)
    }
}
