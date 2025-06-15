use super::mm_list::EMPTY_PAGE;
use super::paging::AllocZeroed as _;
use super::{AsMemoryBlock, Mapping, Page, Permission};
use crate::io::ByteBuffer;
use crate::KResult;
use core::{borrow::Borrow, cell::UnsafeCell, cmp::Ordering};
use eonix_mm::address::{AddrOps as _, VAddr, VRange};
use eonix_mm::page_table::{PageAttribute, RawAttribute, PTE};
use eonix_mm::paging::PFN;

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

    /// # Safety
    /// This function should be called only when we can guarantee that the range
    /// won't overlap with any other range in some scope.
    pub fn grow(&self, count: usize) {
        let range = unsafe { self.range.get().as_mut().unwrap() };
        range.clone_from(&self.range_borrow().grow(count));
    }

    pub fn split(mut self, at: VAddr) -> (Option<Self>, Option<Self>) {
        assert!(at.is_page_aligned());

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

    pub fn handle_cow(&self, pfn: &mut PFN, attr: &mut PageAttribute) {
        assert!(attr.contains(PageAttribute::COPY_ON_WRITE));

        attr.remove(PageAttribute::COPY_ON_WRITE);
        attr.set(PageAttribute::WRITE, self.permission.write);

        let page = unsafe { Page::from_raw(*pfn) };
        if page.is_exclusive() {
            // SAFETY: This is actually safe. If we read `1` here and we have `MMList` lock
            // held, there couldn't be neither other processes sharing the page, nor other
            // threads making the page COW at the same time.
            core::mem::forget(page);
            return;
        }

        let new_page;
        if *pfn == EMPTY_PAGE.pfn() {
            new_page = Page::zeroed();
        } else {
            new_page = Page::alloc();

            unsafe {
                // SAFETY: `page` is CoW, which means that others won't write to it.
                let old_page_data = page.as_memblk().as_bytes();

                // SAFETY: `new_page` is exclusive owned by us.
                let new_page_data = new_page.as_memblk().as_bytes_mut();

                new_page_data.copy_from_slice(old_page_data);
            };
        }

        attr.remove(PageAttribute::ACCESSED);
        *pfn = new_page.into_raw();
    }

    /// # Arguments
    /// * `offset`: The offset from the start of the mapping, aligned to 4KB boundary.
    pub fn handle_mmap(
        &self,
        pfn: &mut PFN,
        attr: &mut PageAttribute,
        offset: usize,
    ) -> KResult<()> {
        // TODO: Implement shared mapping
        let Mapping::File(mapping) = &self.mapping else {
            panic!("Anonymous mapping should not be PA_MMAP");
        };

        assert!(offset < mapping.length, "Offset out of range");
        unsafe {
            Page::with_raw(*pfn, |page| {
                // SAFETY: `page` is marked as mapped, so others trying to read or write to
                //         it will be blocked and enter the page fault handler, where they will
                //         be blocked by the mutex held by us.
                let page_data = page.as_memblk().as_bytes_mut();

                let cnt_to_read = (mapping.length - offset).min(0x1000);
                let cnt_read = mapping.file.read(
                    &mut ByteBuffer::new(&mut page_data[..cnt_to_read]),
                    mapping.offset + offset,
                )?;

                page_data[cnt_read..].fill(0);

                KResult::Ok(())
            })?;
        }

        attr.insert(PageAttribute::PRESENT);
        attr.remove(PageAttribute::MAPPED);
        Ok(())
    }

    pub fn handle(&self, pte: &mut impl PTE, offset: usize) -> KResult<()> {
        let mut attr = pte.get_attr().as_page_attr().expect("Not a page attribute");
        let mut pfn = pte.get_pfn();

        if attr.contains(PageAttribute::COPY_ON_WRITE) {
            self.handle_cow(&mut pfn, &mut attr);
        }

        if attr.contains(PageAttribute::MAPPED) {
            self.handle_mmap(&mut pfn, &mut attr, offset)?;
        }

        attr.set(PageAttribute::ACCESSED, true);
        pte.set(pfn, attr.into());

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
