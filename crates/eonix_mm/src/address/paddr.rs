use core::fmt;
use core::ops::{Add, Sub};
use core::ptr::NonNull;

use super::addr::Addr;
use crate::paging::{PAGE_SIZE_BITS, PFN};

/// Convert PAddr to VAddr.
pub trait PhysAccess {
    /// Translate the data that this address is pointing to into kernel
    /// accessible pointer. Use it with care.
    ///
    /// # Panic
    /// If the address is not properly aligned.
    ///
    /// # Safety
    /// The caller must ensure that the data is of type `T`.
    /// Otherwise, it may lead to undefined behavior.
    unsafe fn as_ptr<T>(paddr: PAddr) -> NonNull<T>;

    /// Translate the kernel accessible pointer back into a physical address.
    ///
    /// # Panic
    /// If the pointer is not properly aligned.
    ///
    /// # Safety
    /// The caller must ensure that the pointer is valid and points to a
    /// valid physical memory location.
    unsafe fn from_ptr<T>(ptr: NonNull<T>) -> PAddr;
}

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
