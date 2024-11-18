use core::{
    cmp::Ordering,
    fmt::{self, Debug, Formatter},
    ops::{Add, RangeBounds, Sub},
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VAddr(pub usize);

#[derive(Clone, Copy)]
pub struct VRange {
    start: VAddr,
    end: VAddr,
}

const USER_SPACE_MEMORY_TOP: VAddr = VAddr(0x8000_0000_0000);

impl VAddr {
    pub fn floor(&self) -> Self {
        VAddr(self.0 & !0xfff)
    }

    pub fn ceil(&self) -> Self {
        VAddr((self.0 + 0xfff) & !0xfff)
    }

    pub fn is_user(&self) -> bool {
        self.0 != 0 && self < &USER_SPACE_MEMORY_TOP
    }
}

impl Sub for VAddr {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl Add<usize> for VAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        VAddr(self.0 + rhs)
    }
}

impl Sub<usize> for VAddr {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        VAddr(self.0 - rhs)
    }
}

impl Debug for VAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "V{:#x}", self.0)
    }
}

impl Debug for VRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}, {:?})", self.start, self.end)
    }
}

impl Eq for VRange {}
impl PartialOrd for VRange {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for VRange {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

/// Any two ranges that have one of them containing the other are considered equal.
impl Ord for VRange {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.start == other.start {
            return Ordering::Equal;
        }

        if self.end == other.end {
            if self.start == self.end {
                return Ordering::Greater;
            }
            if other.start == other.end {
                return Ordering::Less;
            }
            return Ordering::Equal;
        }

        if self.start < other.start {
            if other.end < self.end {
                return Ordering::Equal;
            } else {
                return Ordering::Less;
            }
        }

        if other.start < self.start {
            if self.end < other.end {
                return Ordering::Equal;
            } else {
                return Ordering::Greater;
            }
        }

        unreachable!()
    }
}

impl From<VAddr> for VRange {
    fn from(addr: VAddr) -> Self {
        VRange::new(addr, addr)
    }
}

impl VRange {
    pub fn new(start: VAddr, end: VAddr) -> Self {
        assert!(start <= end);
        VRange { start, end }
    }

    pub fn is_overlapped(&self, other: &Self) -> bool {
        self == other
    }

    pub fn is_user(&self) -> bool {
        self.start < USER_SPACE_MEMORY_TOP && self.end <= USER_SPACE_MEMORY_TOP
    }

    pub fn start(&self) -> VAddr {
        self.start
    }

    pub fn end(&self) -> VAddr {
        self.end
    }

    pub fn len(&self) -> usize {
        self.end.0 - self.start.0
    }

    pub fn shrink(&self, count: usize) -> Self {
        assert!(count <= self.len());
        VRange::new(self.start, self.end - count)
    }

    pub fn grow(&self, count: usize) -> Self {
        VRange::new(self.start, self.end + count)
    }

    pub fn into_range(self) -> impl RangeBounds<Self> {
        VRange::from(self.start())..VRange::from(self.end())
    }
}
