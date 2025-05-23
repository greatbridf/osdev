use super::paging::AllocZeroed as _;
use super::{AsMemoryBlock, Mapping, Page, Permission};
use crate::io::ByteBuffer;
use crate::KResult;
use core::{borrow::Borrow, cell::UnsafeCell, cmp::Ordering};
use eonix_mm::address::{AddrOps as _, VAddr, VRange};
use eonix_mm::page_table::{PageAttribute, PTE};
use eonix_mm::paging::PFN;
use super::mm_list::EMPTY_PAGE;

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

    /// # Return
    /// Whether the whole handling process is done.
    pub fn handle_cow(&self, pte: &mut impl PTE) -> bool {
        let mut page_attr = pte.get_attr();
        let pfn = pte.get_pfn();

        page_attr = page_attr.copy_on_write(false);
        page_attr = page_attr.write(self.permission.write);

        let page = unsafe { Page::from_raw(pfn) };
        if page.is_exclusive() {
            // SAFETY: This is actually safe. If we read `1` here and we have `MMList` lock
            // held, there couldn't be neither other processes sharing the page, nor other
            // threads making the page COW at the same time.
            pte.set_attr(page_attr);
            core::mem::forget(page);
            return true;
        }

        let new_page;
        if is_anonymous(pfn) {
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

        page_attr = page_attr.accessed(false);

        pte.set(new_page.into_raw(), page_attr);

        false
    }

    /// # Arguments
    /// * `offset`: The offset from the start of the mapping, aligned to 4KB boundary.
    pub fn handle_mmap(&self, pte: &mut impl PTE, offset: usize) -> KResult<()> {
        // TODO: Implement shared mapping
        let mut page_attr = pte.get_attr();
        let pfn = pte.get_pfn();

        match &self.mapping {
            Mapping::File(mapping) if offset < mapping.length => {
                let page = unsafe {
                    // SAFETY: Since we are here, the `pfn` must refer to a valid buddy page.
                    Page::with_raw(pfn, |page| page.clone())
                };

                let page_data = unsafe {
                    // SAFETY: `page` is marked as mapped, so others trying to read or write to
                    //         it will be blocked and enter the page fault handler, where they will
                    //         be blocked by the mutex held by us.
                    page.as_memblk().as_bytes_mut()
                };

                let cnt_to_read = (mapping.length - offset).min(0x1000);
                let cnt_read = mapping.file.read(
                    &mut ByteBuffer::new(&mut page_data[..cnt_to_read]),
                    mapping.offset + offset,
                )?;

                page_data[cnt_read..].fill(0);
            }
            Mapping::File(_) => panic!("Offset out of range"),
            _ => panic!("Anonymous mapping should not be PA_MMAP"),
        }

        page_attr = page_attr.present(true).mapped(false);
        pte.set_attr(page_attr);
        Ok(())
    }

    pub fn handle(&self, pte: &mut impl PTE, offset: usize) -> KResult<()> {
        let page_attr = pte.get_attr();

        if page_attr.is_copy_on_write() {
            self.handle_cow(pte);
        }

        if page_attr.is_mapped() {
            self.handle_mmap(pte, offset)?;
        }

        Ok(())
    }
}

/// check pfn with EMPTY_PAGE's pfn
fn is_anonymous(pfn: PFN) -> bool {
    let empty_pfn = EMPTY_PAGE.pfn();
    pfn == empty_pfn
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
