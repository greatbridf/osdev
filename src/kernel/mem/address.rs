use core::{
    cmp::Ordering,
    fmt::{self, Debug, Formatter},
    ops::{Add, Sub, RangeBounds},
};

#[repr(C)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct PAddr(pub usize);

#[repr(C)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct VAddr(pub usize);

#[repr(C)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct PFN(pub usize);

#[repr(C)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct VPN(pub usize);

const PAGE_SIZE: usize = 4096;
const PAGE_SIZE_BITS: usize = 12;
const USER_SPACE_MEMORY_TOP: VAddr = VAddr(0x8000_0000_0000);

impl From<PAddr> for usize {
    fn from(v: PAddr) -> Self {
        v.0
    }
}

impl From<PFN> for usize {
    fn from(v: PFN) -> Self {
        v.0
    }
}

impl From<VAddr> for usize {
    fn from(v: VAddr) -> Self {
       v.0 
    }
}

impl From<VPN> for usize {
    fn from(v: VPN) -> Self {
        v.0
    }
}

impl From<usize> for PAddr {
    fn from(v: usize) -> Self {
        Self(v)
    }
}

impl From<usize> for PFN {
    fn from(v: usize) -> Self {
        Self(v)
    }
}

impl From<usize> for VAddr {
    fn from(v: usize) -> Self {
        Self(v)
    }
}

impl From<usize> for VPN {
    fn from(v: usize) -> Self {
        Self(v)
    }
}


impl From<VPN> for VAddr {
    fn from(v: VPN) -> Self {
        Self(v.0 << PAGE_SIZE_BITS)
    }
}

impl From<VAddr> for VPN {
    fn from(v: VAddr) -> Self {
        assert_eq!(v.page_offset(), 0);
        v.floor_vpn()
    }
}

impl From<PAddr> for PFN {
    fn from(v: PAddr) -> Self {
        assert_eq!(v.page_offset(), 0);
        v.floor_pfn()
    }
}

impl From<PFN> for PAddr {
    fn from(v: PFN) -> Self {
        Self(v.0 << PAGE_SIZE_BITS)
    }
}

impl PAddr {
    pub fn floor_pfn(&self) -> PFN {
        PFN(self.0 / PAGE_SIZE)
    }

    pub fn ceil_pfn(&self) -> PFN {
        PFN((self.0 + PAGE_SIZE - 1) / PAGE_SIZE)
    }

    pub fn page_offset(&self) -> usize {
        self.0 & (PAGE_SIZE - 1)
    }

    pub fn is_aligned(&self) -> bool {
        self.page_offset() == 0
    }
}

impl PFN {
    pub fn buddy_pfn(&self, order: u32) -> PFN {
        PFN::from(self.0 ^ (1 << order))
    }

    pub fn combined_pfn(&self, buddy_pfn: PFN) -> PFN {
        PFN::from(self.0 & buddy_pfn.0)
    }
}

impl VAddr {
    pub const NULL: Self = Self(0);

    pub fn floor_vpn(&self) -> VPN {
        VPN(self.0 / PAGE_SIZE)
    }

    pub fn ceil_vpn(&self) -> VPN {
        VPN((self.0 - 1 + PAGE_SIZE) / PAGE_SIZE)
    }

    pub fn page_offset(&self) -> usize {
        self.0 & (PAGE_SIZE - 1)
    }

    pub fn is_aligned(&self) -> bool {
        self.page_offset() == 0
    }

    pub fn is_user(&self) -> bool {
        self.0 != 0 && self < &USER_SPACE_MEMORY_TOP
    }

    pub fn floor(&self) -> Self {
        VAddr(self.0 & !(PAGE_SIZE - 1))
    }

    pub fn ceil(&self) -> Self {
        VAddr((self.0 + (PAGE_SIZE - 1)) & !(PAGE_SIZE - 1))
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

impl Debug for VAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "VAddr{:#x}", self.0)
    }
}

impl Debug for PAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PAddr{:#x}", self.0)
    }
}

impl Add<usize> for PFN {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        PFN(self.0 + rhs)
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

impl Add<usize> for VPN {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        VPN(self.0 + rhs)
    } 
}

impl Sub for VPN {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl Sub<usize> for VPN {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        VPN(self.0 - rhs)
    }
}

impl Debug for VPN {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "VPN{:#x}", self.0)
    }
}

impl Debug for PFN {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PFN{:#x}", self.0)
    }
}

#[derive(Clone, Copy)]
pub struct VRange {
    start: VAddr,
    end: VAddr,
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
        if self.len() == 0 {
            VRange::from(self.start())..=VRange::from(self.start())
        } else {
            VRange::from(self.start())..=VRange::from(self.end() - 1)
        }
    }
}
