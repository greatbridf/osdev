use core::borrow::Borrow;
use core::cell::UnsafeCell;
use core::cmp;

use eonix_mm::address::{AddrOps as _, VAddr, VRange};
use eonix_mm::page_table::{PageAttribute, RawAttribute, PTE};
use eonix_mm::paging::PFN;

use super::mm_list::EMPTY_PAGE;
use super::{Mapping, Page, Permission};
use crate::kernel::mem::page_cache::PageOffset;
use crate::kernel::mem::{CachePage, PageExcl, PageExt};
use crate::prelude::KResult;

#[derive(Debug)]
pub struct MMArea {
    range: UnsafeCell<VRange>,
    pub(super) mapping: Mapping,
    pub(super) permission: Permission,
    pub is_shared: bool,
}

unsafe impl Send for MMArea {}
unsafe impl Sync for MMArea {}

impl Clone for MMArea {
    fn clone(&self) -> Self {
        Self {
            range: UnsafeCell::new(self.range()),
            mapping: self.mapping.clone(),
            permission: self.permission,
            is_shared: self.is_shared,
        }
    }
}

impl MMArea {
    pub fn new(range: VRange, mapping: Mapping, permission: Permission, is_shared: bool) -> Self {
        Self {
            range: range.into(),
            mapping,
            permission,
            is_shared,
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
            cmp::Ordering::Less => (Some(self), None),
            cmp::Ordering::Greater => (None, Some(self)),
            cmp::Ordering::Equal => {
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
                    is_shared: self.is_shared,
                };

                let new_range = self.range_borrow().shrink(self.range_borrow().end() - at);

                *self.range.get_mut() = new_range;
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

        let mut new_page;
        if *pfn == EMPTY_PAGE.pfn() {
            new_page = PageExcl::zeroed();
        } else {
            new_page = PageExcl::alloc();

            unsafe {
                // SAFETY: `page` is CoW, which means that others won't write to it.
                let old_page_data = page.get_bytes_ptr().as_ref();
                let new_page_data = new_page.as_bytes_mut();

                new_page_data.copy_from_slice(old_page_data);
            };
        }

        attr.remove(PageAttribute::ACCESSED);
        *pfn = new_page.into_page().into_raw();
    }

    /// # Arguments
    /// * `offset`: The offset from the start of the mapping, aligned to 4KB boundary.
    pub async fn handle_mmap(
        &self,
        pfn: &mut PFN,
        attr: &mut PageAttribute,
        offset: usize,
        write: bool,
    ) -> KResult<()> {
        let Mapping::File(file_mapping) = &self.mapping else {
            panic!("Anonymous mapping should not be PA_MMAP");
        };

        assert!(offset < file_mapping.length, "Offset out of range");

        let file_offset = file_mapping.offset + offset;

        let map_page = |page: &Page, cache_page: &CachePage| {
            if !self.permission.write {
                assert!(!write, "Write fault on read-only mapping");

                *pfn = page.clone().into_raw();
                return;
            }

            if self.is_shared {
                // We don't process dirty flags in write faults.
                // Simply assume that page will eventually be dirtied.
                // So here we can set the dirty flag now.
                cache_page.set_dirty(true);
                attr.insert(PageAttribute::WRITE);
                *pfn = page.clone().into_raw();
                return;
            }

            if !write {
                // Delay the copy-on-write until write fault happens.
                attr.insert(PageAttribute::COPY_ON_WRITE);
                *pfn = page.clone().into_raw();
                return;
            }

            // XXX: Change this. Let's handle mapped pages before CoW pages.
            // Nah, we are writing to a mapped private mapping...
            let mut new_page = PageExcl::zeroed();
            new_page
                .as_bytes_mut()
                .copy_from_slice(page.lock().as_bytes());

            attr.insert(PageAttribute::WRITE);
            *pfn = new_page.into_page().into_raw();
        };

        file_mapping
            .page_cache
            .with_page(PageOffset::from_byte_floor(file_offset), map_page)
            .await?;

        attr.insert(PageAttribute::PRESENT);
        attr.remove(PageAttribute::MAPPED);
        Ok(())
    }

    pub async fn handle(&self, pte: &mut impl PTE, offset: usize, write: bool) -> KResult<()> {
        let mut attr = pte.get_attr().as_page_attr().expect("Not a page attribute");
        let mut pfn = pte.get_pfn();

        if attr.contains(PageAttribute::COPY_ON_WRITE) {
            self.handle_cow(&mut pfn, &mut attr);
        }

        if attr.contains(PageAttribute::MAPPED) {
            self.handle_mmap(&mut pfn, &mut attr, offset, write).await?;
        }

        attr.insert(PageAttribute::ACCESSED);

        if write {
            attr.insert(PageAttribute::DIRTY);
        }

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
