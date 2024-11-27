use core::{borrow::Borrow, cell::UnsafeCell, cmp::Ordering};

use super::{Mapping, Permission, VAddr, VRange};

#[derive(Debug)]
pub struct MMArea {
    range: UnsafeCell<VRange>,
    pub(super) mapping: Mapping,
    pub(super) permission: Permission,
}

impl Clone for MMArea {
    fn clone(&self) -> Self {
        Self {
            range: UnsafeCell::new(self.range()),
            mapping: self.mapping.clone(),
            permission: self.permission,
        }
    }
}

impl MMArea {
    pub fn new(range: VRange, mapping: Mapping, permission: Permission) -> Self {
        Self {
            range: range.into(),
            mapping,
            permission,
        }
    }

    fn range_borrow(&self) -> &VRange {
        // SAFETY: The only way we get a reference to `MMArea` object is through `MMListInner`.
        // And `MMListInner` is locked with IRQ disabled.
        unsafe { self.range.get().as_ref().unwrap() }
    }

    pub fn range(&self) -> VRange {
        *self.range_borrow()
    }

    pub fn len(&self) -> usize {
        self.range_borrow().len()
    }

    /// # Safety
    /// This function should be called only when we can guarantee that the range
    /// won't overlap with any other range in some scope.
    pub fn grow(&self, count: usize) {
        let range = unsafe { self.range.get().as_mut().unwrap() };
        range.clone_from(&self.range_borrow().grow(count));
    }

    pub fn split(mut self, at: VAddr) -> (Option<Self>, Option<Self>) {
        assert_eq!(at.floor(), at);

        match self.range_borrow().cmp(&VRange::from(at)) {
            Ordering::Less => (Some(self), None),
            Ordering::Greater => (None, Some(self)),
            Ordering::Equal => {
                let diff = at - self.range_borrow().start();
                if diff == 0 {
                    return (None, Some(self));
                }

                let right = Self {
                    range: VRange::new(at, self.range_borrow().end()).into(),
                    permission: self.permission,
                    mapping: match &self.mapping {
                        Mapping::Anonymous => Mapping::Anonymous,
                        Mapping::File(mapping) => Mapping::File(mapping.offset(diff)),
                    },
                };

                self.range.get_mut().shrink(diff);
                (Some(self), Some(right))
            }
        }
    }
}

impl Eq for MMArea {}
impl PartialEq for MMArea {
    fn eq(&self, other: &Self) -> bool {
        self.range_borrow().eq(other.range_borrow())
    }
}
impl PartialOrd for MMArea {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.range_borrow().partial_cmp(other.range_borrow())
    }
}
impl Ord for MMArea {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.range_borrow().cmp(other.range_borrow())
    }
}

impl Borrow<VRange> for MMArea {
    fn borrow(&self) -> &VRange {
        self.range_borrow()
    }
}
