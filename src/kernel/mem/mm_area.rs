use crate::prelude::*;

use bindings::PA_MMAP;

use core::{borrow::Borrow, cell::UnsafeCell, cmp::Ordering};

use crate::bindings::root::{PA_A, PA_ANON, PA_COW, PA_P, PA_RW};

use super::{Mapping, Page, PageBuffer, Permission, VAddr, VRange, PTE};

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

    #[allow(dead_code)]
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

    /// # Return
    /// Whether the whole handling process is done.
    pub fn handle_cow(&self, pte: &mut PTE) -> bool {
        let mut attributes = pte.attributes();
        let mut pfn = pte.pfn();

        attributes &= !PA_COW as usize;
        if self.permission.write {
            attributes |= PA_RW as usize;
        } else {
            attributes &= !PA_RW as usize;
        }

        let page = unsafe { Page::take_pfn(pfn, 0) };
        if unsafe { page.load_refcount() } == 1 {
            // SAFETY: This is actually safe. If we read `1` here and we have `MMList` lock
            // held, there couldn't be neither other processes sharing the page, nor other
            // threads making the page COW at the same time.
            pte.set_attributes(attributes);
            core::mem::forget(page);
            return true;
        }

        let new_page = Page::alloc_one();
        if attributes & PA_ANON as usize != 0 {
            new_page.zero();
        } else {
            new_page.as_mut_slice().copy_from_slice(page.as_slice());
        }

        attributes &= !(PA_A | PA_ANON) as usize;

        pfn = new_page.into_pfn();
        pte.set(pfn, attributes);

        false
    }

    /// # Arguments
    /// * `offset`: The offset from the start of the mapping, aligned to 4KB boundary.
    pub fn handle_mmap(&self, pte: &mut PTE, offset: usize) -> KResult<()> {
        // TODO: Implement shared mapping
        let mut attributes = pte.attributes();
        let pfn = pte.pfn();

        attributes |= PA_P as usize;

        match &self.mapping {
            Mapping::File(mapping) if offset < mapping.length => {
                // SAFETY: Since we are here, the `pfn` must refer to a valid buddy page.
                let page = unsafe { Page::from_pfn(pfn, 0) };
                let nread = mapping
                    .file
                    .read(&mut PageBuffer::new(page.clone()), mapping.offset + offset)?;

                if nread < page.len() {
                    page.as_mut_slice()[nread..].fill(0);
                }

                if mapping.length - offset < 0x1000 {
                    let length_to_end = mapping.length - offset;
                    page.as_mut_slice()[length_to_end..].fill(0);
                }
            }
            Mapping::File(_) => panic!("Offset out of range"),
            _ => panic!("Anonymous mapping should not be PA_MMAP"),
        }

        attributes &= !PA_MMAP as usize;
        pte.set_attributes(attributes);
        Ok(())
    }

    pub fn handle(&self, pte: &mut PTE, offset: usize) -> KResult<()> {
        if pte.is_cow() {
            self.handle_cow(pte);
        }

        if pte.is_mmap() {
            self.handle_mmap(pte, offset)?;
        }

        Ok(())
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
