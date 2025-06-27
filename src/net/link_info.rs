use core::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LinkId(usize);

#[derive(Debug, Clone, Copy)]
pub enum LinkStatus {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy)]
pub enum LinkSpeed {
    SpeedUnknown,
    SpeedMegs(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Mac([u8; 6]);

pub struct LinkState {
    pub status: LinkStatus,
    pub speed: LinkSpeed,
    pub mac: Mac,
}

impl LinkId {
    pub fn new() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        Self(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

impl LinkState {
    pub fn new() -> Self {
        Self {
            status: LinkStatus::Down,
            speed: LinkSpeed::SpeedUnknown,
            mac: Mac::zeros(),
        }
    }
}

impl Mac {
    pub const fn zeros() -> Self {
        Self([0; 6])
    }

    pub const fn new(mac: [u8; 6]) -> Self {
        Self(mac)
    }
}

impl fmt::Debug for Mac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MAC({self:?})",)
    }
}

impl fmt::Display for Mac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

impl AsRef<[u8; 6]> for Mac {
    fn as_ref(&self) -> &[u8; 6] {
        &self.0
    }
}
