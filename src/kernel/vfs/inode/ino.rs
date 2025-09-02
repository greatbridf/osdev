use core::{
    fmt::{Debug, Display, Formatter},
    sync::atomic::AtomicU64,
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ino(u64);

pub struct AtomicIno(AtomicU64);

impl Ino {
    pub const fn new(ino: u64) -> Self {
        Self(ino)
    }

    pub const fn as_raw(self) -> u64 {
        self.0
    }
}

impl Debug for Ino {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "Ino({})", self.0)
    }
}

impl Display for Ino {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}
