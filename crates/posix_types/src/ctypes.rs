use crate::result::PosixError;

#[cfg(target_arch = "x86_64")]
pub(crate) type ArchPtrType = u32;

#[cfg(not(target_arch = "x86_64"))]
pub(crate) type ArchPtrType = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PtrT(pub(crate) ArchPtrType);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Long(pub(crate) ArchPtrType);

impl PtrT {
    pub fn new(ptr: usize) -> Result<Self, PosixError> {
        ptr.try_into().map(Self).map_err(|_| PosixError::EFAULT)
    }

    pub const fn new_val(ptr: ArchPtrType) -> Self {
        Self(ptr as ArchPtrType)
    }

    pub const fn null() -> Self {
        Self(0)
    }

    pub const fn addr(self) -> usize {
        self.0 as usize
    }

    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
}

impl Long {
    pub const ZERO: Self = Self(0);

    pub fn new(value: usize) -> Result<Self, PosixError> {
        value.try_into().map(Self).map_err(|_| PosixError::EINVAL)
    }

    pub const fn new_val(value: ArchPtrType) -> Self {
        Self(value as ArchPtrType)
    }

    pub const fn zero() -> Self {
        Self(0)
    }

    pub const fn get(self) -> usize {
        self.0 as usize
    }
}
