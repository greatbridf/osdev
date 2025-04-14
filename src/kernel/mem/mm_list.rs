mod page_fault;

use super::{MMArea, Page, PageTable, VAddr, VRange};
use crate::kernel::vfs::dentry::Dentry;
use crate::{prelude::*, sync::ArcSwap};
use alloc::{collections::btree_set::BTreeSet, sync::Arc};
use bindings::{EEXIST, EFAULT, EINVAL, ENOMEM, KERNEL_PML4};
use core::{
    ops::Sub as _,
    sync::atomic::{AtomicUsize, Ordering},
};
use eonix_runtime::task::Task;
use eonix_sync::Mutex;

pub use page_fault::handle_page_fault;

#[derive(Debug, Clone)]
pub struct FileMapping {
    pub file: Arc<Dentry>,
    /// Offset in the file, aligned to 4KB boundary.
    pub offset: usize,
    /// Length of the mapping. Exceeding part will be zeroed.
    pub length: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Permission {
    pub write: bool,
    pub execute: bool,
}

#[derive(Debug, Clone)]
pub enum Mapping {
    Anonymous,
    File(FileMapping),
}

#[derive(Debug)]
struct MMListInner {
    areas: BTreeSet<MMArea>,
    page_table: PageTable,
    break_start: Option<VRange>,
    break_pos: Option<VAddr>,
}

#[derive(Debug)]
pub struct MMList {
    inner: ArcSwap<Mutex<MMListInner>>,
    /// Only used in kernel space to switch page tables on context switch.
    root_page_table: AtomicUsize,
}

impl FileMapping {
    pub fn new(file: Arc<Dentry>, offset: usize, length: usize) -> Self {
        assert_eq!(offset & 0xfff, 0);
        Self {
            file,
            offset,
            length,
        }
    }

    pub fn offset(&self, offset: usize) -> Self {
        if self.length <= offset {
            Self::new(self.file.clone(), self.offset + self.length, 0)
        } else {
            Self::new(
                self.file.clone(),
                self.offset + offset,
                self.length - offset,
            )
        }
    }
}

impl MMListInner {
    fn overlapping_addr(&self, addr: VAddr) -> Option<&MMArea> {
        self.areas.get(&VRange::from(addr))
    }

    fn check_overlapping_addr(&self, addr: VAddr) -> bool {
        addr.is_user() && self.overlapping_addr(addr).is_none()
    }

    fn overlapping_range(&self, range: VRange) -> impl DoubleEndedIterator<Item = &MMArea> + '_ {
        self.areas.range(range.into_range())
    }

    fn check_overlapping_range(&self, range: VRange) -> bool {
        range.is_user() && self.overlapping_range(range).next().is_none()
    }

    fn find_available(&self, hint: VAddr, len: usize) -> Option<VAddr> {
        let mut range = if hint == VAddr::NULL {
            VRange::new(VAddr(0x1234000), VAddr(0x1234000 + len).ceil())
        } else {
            VRange::new(hint.floor(), (hint + len).ceil())
        };
        let len = range.len();

        loop {
            if !range.is_user() {
                return None;
            }

            match self.overlapping_range(range).next_back() {
                None => return Some(range.start()),
                Some(area) => {
                    range = VRange::new(area.range().end().ceil(), area.range().end().ceil() + len);
                }
            }
        }
    }

    fn unmap(&mut self, start: VAddr, len: usize) -> KResult<()> {
        assert_eq!(start.floor(), start);
        let end = (start + len).ceil();
        let range = VRange::new(start, end);
        if !range.is_user() {
            return Err(EINVAL);
        }

        let check_range = VRange::from(range.start())..VRange::from(range.end());
        let mut front_remaining = None;
        let mut back_remaining = None;

        self.areas.retain(|area| {
            if !check_range.contains(&area.range()) {
                return true;
            }
            if area.range() == range.start().into() {
                let (left, right) = area.clone().split(range.start());
                self.page_table.unmap(&right.unwrap());

                if let Some(left) = left {
                    assert!(
                        front_remaining.replace(left).is_none(),
                        "There should be only one `front`."
                    );
                }
            } else if area.range() == range.end().into() {
                let (left, right) = area.clone().split(range.end());
                self.page_table.unmap(&left.unwrap());

                assert!(
                    back_remaining
                        .replace(right.expect("`right` should be valid"))
                        .is_none(),
                    "There should be only one `back`."
                );
            } else {
                self.page_table.unmap(area);
            }

            false
        });

        if let Some(front) = front_remaining {
            self.areas.insert(front);
        }
        if let Some(back) = back_remaining {
            self.areas.insert(back);
        }

        Ok(())
    }

    fn mmap(
        &mut self,
        at: VAddr,
        len: usize,
        mapping: Mapping,
        permission: Permission,
    ) -> KResult<()> {
        assert_eq!(at.floor(), at);
        assert_eq!(len & 0xfff, 0);
        let range = VRange::new(at, at + len);

        // We are doing a area marker insertion.
        if len == 0 && !self.check_overlapping_addr(at) || !self.check_overlapping_range(range) {
            return Err(EEXIST);
        }

        match &mapping {
            Mapping::Anonymous => self.page_table.set_anonymous(range, permission),
            Mapping::File(_) => self.page_table.set_mmapped(range, permission),
        }

        self.areas.insert(MMArea::new(range, mapping, permission));
        Ok(())
    }
}

impl MMList {
    pub fn new() -> Self {
        let page_table = PageTable::new();
        Self {
            root_page_table: AtomicUsize::from(page_table.root_page_table()),
            inner: ArcSwap::new(Mutex::new(MMListInner {
                areas: BTreeSet::new(),
                page_table,
                break_start: None,
                break_pos: None,
            })),
        }
    }

    pub fn new_cloned(&self) -> Self {
        let inner = self.inner.borrow();
        let inner = Task::block_on(inner.lock());

        let page_table = PageTable::new();
        let list = Self {
            root_page_table: AtomicUsize::from(page_table.root_page_table()),
            inner: ArcSwap::new(Mutex::new(MMListInner {
                areas: inner.areas.clone(),
                page_table,
                break_start: inner.break_start,
                break_pos: inner.break_pos,
            })),
        };

        {
            let list_inner = list.inner.borrow();
            let list_inner = Task::block_on(list_inner.lock());

            for area in list_inner.areas.iter() {
                let new_iter = list_inner.page_table.iter_user(area.range()).unwrap();
                let old_iter = inner.page_table.iter_user(area.range()).unwrap();

                for (new, old) in new_iter.zip(old_iter) {
                    new.setup_cow(old);
                }
            }
        }

        // We set some pages as COW, so we need to invalidate TLB.
        inner.page_table.lazy_invalidate_tlb_all();

        list
    }

    pub fn switch_page_table(&self) {
        let root_page_table = self.root_page_table.load(Ordering::Relaxed);
        assert_ne!(root_page_table, 0);
        arch::set_root_page_table(root_page_table);
    }

    pub fn replace(&self, new: Self) {
        // Switch to kernel page table in case we are using the page table to be swapped and released.
        let mut switched = false;
        if arch::get_root_page_table() == self.root_page_table.load(Ordering::Relaxed) {
            arch::set_root_page_table(KERNEL_PML4 as usize);
            switched = true;
        }

        unsafe {
            // SAFETY: Even if we're using the page table, we've switched to kernel page table.
            // So it's safe to release the old memory list.
            self.release();
        }

        // SAFETY: `self.inner` should be `None` after releasing.
        self.inner.swap(Some(new.inner.borrow().clone()));
        self.root_page_table.store(
            new.root_page_table.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );

        if switched {
            self.switch_page_table();
        }
    }

    /// # Safety
    /// This function is unsafe because the caller should make sure that the `inner` is not currently used.
    pub unsafe fn release(&self) {
        // TODO: Check whether we should wake someone up if they've been put to sleep when calling `vfork`.
        self.inner.swap(None);
        self.root_page_table
            .swap(KERNEL_PML4 as _, Ordering::Relaxed);
    }

    /// No need to do invalidation manually, `PageTable` already does it.
    pub fn unmap(&self, start: VAddr, len: usize) -> KResult<()> {
        Task::block_on(self.inner.borrow().lock()).unmap(start, len)
    }

    pub fn mmap_hint(
        &self,
        hint: VAddr,
        len: usize,
        mapping: Mapping,
        permission: Permission,
    ) -> KResult<VAddr> {
        let inner = self.inner.borrow();
        let mut inner = Task::block_on(inner.lock());

        if hint == VAddr::NULL {
            let at = inner.find_available(hint, len).ok_or(ENOMEM)?;
            inner.mmap(at, len, mapping, permission)?;
            return Ok(at);
        }

        match inner.mmap(hint, len, mapping.clone(), permission) {
            Ok(()) => Ok(hint),
            Err(EEXIST) => {
                let at = inner.find_available(hint, len).ok_or(ENOMEM)?;
                inner.mmap(at, len, mapping, permission)?;
                Ok(at)
            }
            Err(err) => Err(err),
        }
    }

    pub fn mmap_fixed(
        &self,
        at: VAddr,
        len: usize,
        mapping: Mapping,
        permission: Permission,
    ) -> KResult<VAddr> {
        Task::block_on(self.inner.borrow().lock())
            .mmap(at, len, mapping.clone(), permission)
            .map(|_| at)
    }

    pub fn set_break(&self, pos: Option<VAddr>) -> VAddr {
        let inner = self.inner.borrow();
        let mut inner = Task::block_on(inner.lock());

        // SAFETY: `set_break` is only called in syscalls, where program break should be valid.
        assert!(inner.break_start.is_some() && inner.break_pos.is_some());
        let break_start = inner.break_start.unwrap();
        let current_break = inner.break_pos.unwrap();
        let pos = match pos {
            None => return current_break,
            Some(pos) => pos.ceil(),
        };

        let range = VRange::new(current_break, pos);
        if !inner.check_overlapping_range(range) {
            return current_break;
        }

        if !inner.areas.contains(&break_start) {
            inner.areas.insert(MMArea::new(
                break_start,
                Mapping::Anonymous,
                Permission {
                    write: true,
                    execute: false,
                },
            ));
        }

        let program_break = inner
            .areas
            .get(&break_start)
            .expect("Program break area should be valid");

        let len: usize = pos.sub(current_break);
        inner.page_table.set_anonymous(
            VRange::from(program_break.range().end()).grow(len),
            Permission {
                write: true,
                execute: false,
            },
        );

        program_break.grow(len);

        inner.break_pos = Some(pos);
        pos
    }

    /// This should be called only **once** for every thread.
    pub fn register_break(&self, start: VAddr) {
        let inner = self.inner.borrow();
        let mut inner = Task::block_on(inner.lock());
        assert!(inner.break_start.is_none() && inner.break_pos.is_none());

        inner.break_start = Some(start.into());
        inner.break_pos = Some(start);
    }

    /// Access the memory area with the given function.
    /// The function will be called with the offset of the area and the slice of the area.
    pub fn access_mut<F>(&self, start: VAddr, len: usize, func: F) -> KResult<()>
    where
        F: Fn(usize, &mut [u8]),
    {
        // First, validate the address range.
        let end = start + len;
        if !start.is_user() || !end.is_user() {
            return Err(EINVAL);
        }

        let inner = self.inner.borrow();
        let inner = Task::block_on(inner.lock());

        let mut offset = 0;
        let mut remaining = len;
        let mut current = start;

        while remaining > 0 {
            let area = inner.overlapping_addr(current).ok_or(EFAULT)?;

            let area_start = area.range().start();
            let area_end = area.range().end();
            let area_remaining = area_end - current;

            let access_len = remaining.min(area_remaining);
            let access_end = current + access_len;

            for (idx, pte) in inner
                .page_table
                .iter_user(VRange::new(current, access_end))?
                .enumerate()
            {
                let page_start = current.floor() + idx * 0x1000;
                let page_end = page_start + 0x1000;

                area.handle(pte, page_start - area_start)?;

                let start_offset;
                if page_start < current {
                    start_offset = current - page_start;
                } else {
                    start_offset = 0;
                }

                let end_offset;
                if page_end > access_end {
                    end_offset = access_end - page_start;
                } else {
                    end_offset = 0x1000;
                }

                unsafe {
                    let page = Page::from_pfn(pte.pfn(), 0);
                    func(
                        offset + idx * 0x1000,
                        &mut page.as_mut_slice()[start_offset..end_offset],
                    );
                }
            }

            offset += access_len;
            remaining -= access_len;
            current = access_end;
        }

        Ok(())
    }
}
