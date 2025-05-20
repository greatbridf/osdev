use super::addr::Addr;
use core::{cmp::Ordering, fmt, ops::RangeBounds};

#[derive(Clone, Copy)]
/// A range of addresses.
///
/// The range is defined by two addresses, `start` and `end` and is inclusive
/// on the start and exclusive on the end.
///
/// # Relations
///
/// ## Comparison
///
/// ### Equal
/// Any two ranges that have one of them **containing** the other are considered equal.
///
/// ### Less
/// If the two are not equal, the one that has the **smallest** start address is considered less.
///
/// ### Greater
/// If the two are not equal, the one that has the **largest** end address is considered greater.
///
/// ## Overlapping Check
/// Use `overlap_with` instead of `==` to check if two ranges overlap.
pub struct AddrRange<A: Addr> {
    start: A,
    end: A,
}

impl<A: Addr> Eq for AddrRange<A> {}
impl<A: Addr> PartialOrd for AddrRange<A> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<A: Addr> PartialEq for AddrRange<A> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl<A: Addr> Ord for AddrRange<A> {
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

impl<A: Addr> From<A> for AddrRange<A> {
    fn from(addr: A) -> Self {
        Self {
            start: addr,
            end: addr,
        }
    }
}

impl<A: Addr> AddrRange<A> {
    /// Creates a new `AddrRange` with the given start and end addresses.
    ///
    /// # Panics
    /// Panics if the start address is greater than the end address.
    ///
    /// # Hint
    /// Use `AddrRange::from(addr).grow(size)` to create a range of size `size`
    /// starting from `addr`.
    pub fn new(start: A, end: A) -> Self {
        assert!(start <= end);
        Self { start, end }
    }

    pub const fn start(&self) -> A {
        self.start
    }

    pub const fn end(&self) -> A {
        self.end
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn shrink(&self, size: usize) -> Self {
        assert!(size <= self.len());
        Self::new(self.start, self.end - size)
    }

    pub fn grow(&self, count: usize) -> Self {
        Self::new(self.start, self.end + count)
    }

    pub fn into_bounds(&self) -> impl RangeBounds<Self> {
        if self.len() == 0 {
            Self::from(self.start())..=Self::from(self.start())
        } else {
            Self::from(self.start())..=Self::from(self.end() - 1)
        }
    }

    pub fn overlap_with(&self, other: &Self) -> bool {
        self.start < other.end && self.end > other.start
    }

    pub fn split_at_checked(&self, at: A) -> (Option<Self>, Option<Self>) {
        if self.end <= at {
            (Some(*self), None)
        } else if at <= self.start {
            (None, Some(*self))
        } else {
            (
                Some(Self::new(self.start, at)),
                Some(Self::new(at, self.end)),
            )
        }
    }

    pub fn split_at(&self, at: A) -> (Self, Self) {
        let (left, right) = self.split_at_checked(at);
        (
            left.expect("`at` is too large"),
            right.expect("`at` is too small"),
        )
    }

    pub fn mask_with_checked(&self, mask: &Self) -> Option<(Option<Self>, Self, Option<Self>)> {
        if mask.len() == 0 || !self.overlap_with(mask) {
            return None;
        }

        let left;
        let mut mid;
        let right;

        if self.start < mask.start && mask.start < self.end {
            let (l, r) = self.split_at(mask.start);
            left = Some(l);
            mid = r;
        } else {
            left = None;
            mid = *self;
        }

        if mask.end < self.end {
            let (l, r) = mid.split_at(mask.end);
            mid = l;
            right = Some(r);
        } else {
            right = None;
        }

        Some((left, mid, right))
    }
}

impl<A: Addr + fmt::Debug> fmt::Debug for AddrRange<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}, {:?})", self.start, self.end)
    }
}
