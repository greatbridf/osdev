use super::mm_list::EMPTY_PAGE;
use super::paging::AllocZeroed as _;
use super::{AsMemoryBlock, Mapping, Page, Permission};
use crate::kernel::constants::EINVAL;
use crate::prelude::KResult;
use core::borrow::Borrow;
use core::cell::UnsafeCell;
use core::cmp;
use eonix_mm::address::{AddrOps as _, VAddr, VRange};
use eonix_mm::page_table::{PageAttribute, RawAttribute, PTE};
use eonix_mm::paging::{PAGE_SIZE, PFN};
use eonix_runtime::task::Task;

#[derive(Debug)]
pub struct MMArea {
    range: UnsafeCell<VRange>,
    pub(super) mapping: Mapping,
    pub(super) permission: Permission,
    pub is_shared: bool,
}

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

        let Some(page_cache) = file_mapping.file.page_cache() else {
            panic!("Mapping file should have pagecache");
        };

        let file_offset = file_mapping.offset + offset;
        let cnt_to_read = (file_mapping.length - offset).min(0x1000);

        page_cache
            .with_page(file_offset, |page, cache_page| {
                // Non-write faults: we find page in pagecache and do mapping
                // Write fault: we need to care about shared or private mapping.
                if !write {
                    // Bss is embarrassing in pagecache!
                    // We have to assume cnt_to_read < PAGE_SIZE all bss
                    if cnt_to_read < PAGE_SIZE {
                        let new_page = Page::zeroed();
                        unsafe {
                            let page_data = new_page.as_memblk().as_bytes_mut();
                            page_data[..cnt_to_read]
                                .copy_from_slice(&page.as_memblk().as_bytes()[..cnt_to_read]);
                        }
                        *pfn = new_page.into_raw();
                    } else {
                        *pfn = page.clone().into_raw();
                    }

                    if self.permission.write {
                        if self.is_shared {
                            // The page may will not be written,
                            // But we simply assume page will be dirty
                            cache_page.set_dirty();
                            attr.insert(PageAttribute::WRITE);
                        } else {
                            attr.insert(PageAttribute::COPY_ON_WRITE);
                        }
                    }
                } else {
                    if self.is_shared {
                        cache_page.set_dirty();
                        *pfn = page.clone().into_raw();
                    } else {
                        let new_page = Page::zeroed();
                        unsafe {
                            let page_data = new_page.as_memblk().as_bytes_mut();
                            page_data[..cnt_to_read]
                                .copy_from_slice(&page.as_memblk().as_bytes()[..cnt_to_read]);
                        }
                        *pfn = new_page.into_raw();
                    }

                    attr.insert(PageAttribute::WRITE);
                }
            })
            .await?
            .ok_or(EINVAL)?;

        attr.insert(PageAttribute::PRESENT);
        attr.remove(PageAttribute::MAPPED);
        Ok(())
    }

    pub fn handle(&self, pte: &mut impl PTE, offset: usize, write: bool) -> KResult<()> {
        let mut attr = pte.get_attr().as_page_attr().expect("Not a page attribute");
        let mut pfn = pte.get_pfn();

        if attr.contains(PageAttribute::COPY_ON_WRITE) {
            self.handle_cow(&mut pfn, &mut attr);
        }

        if attr.contains(PageAttribute::MAPPED) {
            Task::block_on(self.handle_mmap(&mut pfn, &mut attr, offset, write))?;
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
