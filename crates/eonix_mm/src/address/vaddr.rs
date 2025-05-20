use super::addr::Addr;
use core::{
    fmt,
    ops::{Add, Sub},
};

#[repr(transparent)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct VAddr(usize);

impl From<usize> for VAddr {
    fn from(v: usize) -> Self {
        Self::from(v)
    }
}

impl VAddr {
    pub const NULL: Self = Self(0);

    pub const fn from(v: usize) -> Self {
        Self(v)
    }
}

impl Sub for VAddr {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl Sub<usize> for VAddr {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        VAddr(self.0 - rhs)
    }
}

impl Add<usize> for VAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        VAddr(self.0 + rhs)
    }
}

impl fmt::Debug for VAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VAddr({:#x})", self.0)
    }
}

impl Addr for VAddr {
    fn addr(self) -> usize {
        let Self(addr) = self;
        addr
    }
}
