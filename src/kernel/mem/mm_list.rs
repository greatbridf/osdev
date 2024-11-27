mod page_fault;

use crate::prelude::*;

use alloc::{collections::btree_set::BTreeSet, sync::Arc};
use bindings::{EEXIST, EINVAL, ENOMEM};

use crate::kernel::vfs::dentry::Dentry;

use super::{MMArea, PageTable, VAddr, VRange};

pub use page_fault::{handle_page_fault, PageFaultError};

#[derive(Debug, Clone)]
pub struct FileMapping {
    file: Arc<Dentry>,
    /// Offset in the file, aligned to 4KB boundary.
    offset: usize,
    /// Length of the mapping. Exceeding part will be zeroed.
    length: usize,
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
    break_start: Option<VRange>,
    break_pos: Option<VAddr>,
}

#[derive(Debug)]
pub struct MMList {
    /// # Safety
    /// This field might be used in IRQ context, so it should be locked with `lock_irq()`.
    inner: Mutex<MMListInner>,
    /// Do not modify entries in the page table without acquiring the `inner` lock.
    page_table: PageTable,
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

    fn unmap(&mut self, page_table: &PageTable, start: VAddr, len: usize) -> KResult<()> {
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
                page_table.unmap(&right.unwrap());

                if let Some(left) = left {
                    assert!(
                        front_remaining.replace(left).is_none(),
                        "There should be only one `front`."
                    );
                }
            } else if area.range() == range.end().into() {
                let (left, right) = area.clone().split(range.end());
                page_table.unmap(&left.unwrap());

                assert!(
                    back_remaining
                        .replace(right.expect("`right` should be valid"))
                        .is_none(),
                    "There should be only one `back`."
                );
            } else {
                page_table.unmap(area);
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
        page_table: &PageTable,
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
            Mapping::Anonymous => page_table.set_anonymous(range, permission),
            Mapping::File(_) => page_table.set_mmapped(range, permission),
        }

        self.areas.insert(MMArea::new(range, mapping, permission));
        Ok(())
    }
}

impl MMList {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(MMListInner {
                areas: BTreeSet::new(),
                break_start: None,
                break_pos: None,
            }),
            page_table: PageTable::new(),
        })
    }

    pub fn new_cloned(&self) -> Arc<Self> {
        let inner = self.inner.lock_irq();

        let list = Arc::new(Self {
            inner: Mutex::new(MMListInner {
                areas: inner.areas.clone(),
                break_start: inner.break_start,
                break_pos: inner.break_pos,
            }),
            page_table: PageTable::new(),
        });

        // SAFETY: `self.inner` already locked with IRQ disabled.
        {
            let list_inner = list.inner.lock();

            for area in list_inner.areas.iter() {
                let new_iter = list.page_table.iter_user(area.range()).unwrap();
                let old_iter = self.page_table.iter_user(area.range()).unwrap();

                for (new, old) in new_iter.zip(old_iter) {
                    new.setup_cow(old);
                }
            }
        }

        // We set some pages as COW, so we need to invalidate TLB.
        self.page_table.lazy_invalidate_tlb_all();

        list
    }

    /// No need to do invalidation manually, `PageTable` already does it.
    pub fn clear_user(&self) {
        let mut inner = self.inner.lock_irq();
        inner.areas.retain(|area| {
            self.page_table.unmap(area);
            false
        });
        inner.break_start = None;
        inner.break_pos = None;
    }

    pub fn switch_page_table(&self) {
        self.page_table.switch();
    }

    /// No need to do invalidation manually, `PageTable` already does it.
    pub fn unmap(&self, start: VAddr, len: usize) -> KResult<()> {
        self.inner.lock_irq().unmap(&self.page_table, start, len)
    }

    pub fn mmap_hint(
        &self,
        hint: VAddr,
        len: usize,
        mapping: Mapping,
        permission: Permission,
    ) -> KResult<VAddr> {
        let mut inner = self.inner.lock_irq();
        if hint == VAddr::NULL {
            let at = inner.find_available(hint, len).ok_or(ENOMEM)?;
            inner.mmap(&self.page_table, at, len, mapping, permission)?;
            return Ok(at);
        }

        match inner.mmap(&self.page_table, hint, len, mapping.clone(), permission) {
            Ok(()) => Ok(hint),
            Err(EEXIST) => {
                let at = inner.find_available(hint, len).ok_or(ENOMEM)?;
                inner.mmap(&self.page_table, at, len, mapping, permission)?;
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
        self.inner
            .lock_irq()
            .mmap(&self.page_table, at, len, mapping.clone(), permission)
            .map(|_| at)
    }

    pub fn set_break(&self, pos: Option<VAddr>) -> VAddr {
        let mut inner = self.inner.lock_irq();

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

        let len = pos - current_break;
        self.page_table.set_anonymous(
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
        let mut inner = self.inner.lock_irq();
        assert!(inner.break_start.is_none() && inner.break_pos.is_none());

        inner.break_start = Some(start.into());
        inner.break_pos = Some(start);
    }
}

impl Drop for MMList {
    fn drop(&mut self) {
        self.clear_user();
    }
}
