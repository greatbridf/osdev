use core::fmt::{Debug, Display, Formatter};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId {
    pub major: u16,
    pub minor: u16,
}

impl DeviceId {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    pub const fn from_raw(raw: u32) -> Self {
        Self {
            major: (raw >> 16) as u16,
            minor: (raw & 0xFFFF) as u16,
        }
    }

    pub const fn to_raw(self) -> u32 {
        ((self.major as u32) << 16) | (self.minor as u32)
    }
}

impl Debug for DeviceId {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "DeviceId({:04x}:{:04x})", self.major, self.minor)
    }
}

impl Display for DeviceId {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:04x}:{:04x}", self.major, self.minor)
    }
}
