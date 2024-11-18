use crate::prelude::*;

use alloc::{collections::btree_set::BTreeSet, sync::Arc};
use bindings::{EEXIST, EINVAL, ENOMEM};

use crate::kernel::vfs::dentry::Dentry;

use super::{MMArea, PageTable, VAddr, VRange};

#[derive(Debug, Clone)]
pub struct FileMapping {
    file: Arc<Dentry>,
    offset: usize,
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
    /// # Safety
    /// This field might be used in IRQ context, so it should be locked with `lock_irq()`.
    inner: Spin<MMListInner>,
}

impl FileMapping {
    pub fn new(file: Arc<Dentry>, offset: usize) -> Self {
        assert_eq!(offset & 0xfff, 0);
        Self { file, offset }
    }

    pub fn offset(&self, offset: usize) -> Self {
        Self::new(self.file.clone(), self.offset + offset)
    }
}

impl MMListInner {
    fn clear_user(&mut self) {
        self.areas.retain(|area| {
            self.page_table.unmap(area);
            false
        });
        self.break_start = None;
        self.break_pos = None;
    }

    fn overlapping_addr(&self, addr: VAddr) -> Option<&MMArea> {
        self.areas.get(&VRange::from(addr))
    }

    fn check_overlapping_addr(&self, addr: VAddr) -> bool {
        addr.is_user() && self.overlapping_addr(addr).is_some()
    }

    fn overlapping_range(&self, range: VRange) -> impl DoubleEndedIterator<Item = &MMArea> + '_ {
        self.areas.range(range.into_range())
    }

    fn check_overlapping_range(&self, range: VRange) -> bool {
        range.is_user() && self.overlapping_range(range).next().is_some()
    }

    fn find_available(&self, hint: VAddr, len: usize) -> Option<VAddr> {
        let mut range = VRange::new(hint.floor(), (hint + len).ceil());
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

    fn set_break(&mut self, pos: Option<VAddr>) -> VAddr {
        // SAFETY: `set_break` is only called in syscalls, where program break should be valid.
        assert!(self.break_start.is_some() && self.break_pos.is_some());
        let break_start = self.break_start.unwrap();
        let current_break = self.break_pos.unwrap();
        let pos = match pos {
            None => return current_break,
            Some(pos) => pos.ceil(),
        };

        let range = VRange::new(current_break, pos);
        if !self.check_overlapping_range(range) {
            return current_break;
        }

        self.page_table.set_anonymous(
            range,
            Permission {
                write: true,
                execute: false,
            },
        );

        let program_break = self
            .areas
            .get(&break_start)
            .expect("Program break area should be valid");
        program_break.grow(pos - current_break);

        self.break_pos = Some(pos);
        pos
    }
}

impl MMList {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Spin::new(MMListInner {
                areas: BTreeSet::new(),
                page_table: PageTable::new(),
                break_start: None,
                break_pos: None,
            }),
        })
    }

    /// # Safety
    /// Calling this function on the `MMList` of current process will need invalidating
    /// the TLB cache after the clone will be done. We might set some of the pages as COW.
    pub fn new_cloned(&self) -> Arc<Self> {
        let inner = self.inner.lock_irq();

        let list = Arc::new(Self {
            inner: Spin::new(MMListInner {
                areas: inner.areas.clone(),
                page_table: PageTable::new(),
                break_start: inner.break_start,
                break_pos: inner.break_pos,
            }),
        });

        // SAFETY: `self.inner` already locked with IRQ disabled.
        {
            let list_inner = list.inner.lock();

            for area in list_inner.areas.iter() {
                let new_iter = list_inner.page_table.iter_user(area.range());
                let old_iter = inner.page_table.iter_user(area.range());

                for (new, old) in new_iter.zip(old_iter) {
                    new.setup_cow(old);
                }
            }
        }

        list
    }

    /// # Safety
    /// Calling this function on the `MMList` of current process will need invalidating
    /// the TLB cache after the clone will be done. We might remove some mappings.
    pub fn clear_user(&self) {
        self.inner.lock_irq().clear_user()
    }

    pub fn switch_page_table(&self) {
        self.inner.lock_irq().page_table.switch();
    }

    /// No need to do invalidation manually, `PageTable` already does it.
    pub fn unmap(&self, start: VAddr, len: usize) -> KResult<()> {
        self.inner.lock_irq().unmap(start, len)
    }

    pub fn mmap(
        &self,
        at: VAddr,
        len: usize,
        mapping: Mapping,
        permission: Permission,
        fixed: bool,
    ) -> KResult<VAddr> {
        let mut inner = self.inner.lock_irq();
        match inner.mmap(at, len, mapping.clone(), permission) {
            Ok(()) => Ok(at),
            Err(EEXIST) if fixed => Err(EEXIST),
            Err(EEXIST) => {
                let at = inner.find_available(at, len).ok_or(ENOMEM)?;
                inner.mmap(at, len, mapping, permission)?;
                Ok(at)
            }
            Err(err) => Err(err),
        }
    }

    pub fn set_break(&self, pos: Option<VAddr>) -> VAddr {
        self.inner.lock_irq().set_break(pos)
    }

    pub fn register_break(&self, start: VAddr) {
        let mut inner = self.inner.lock_irq();
        assert!(inner.break_start.is_none() && inner.break_pos.is_none());

        inner
            .mmap(
                start,
                0,
                Mapping::Anonymous,
                Permission {
                    write: true,
                    execute: false,
                },
            )
            .expect("Probably, we have a bug in the ELF loader?");

        inner.break_start = Some(start.into());
        inner.break_pos = Some(start);
    }
}

impl Drop for MMList {
    fn drop(&mut self) {
        self.clear_user();
    }
}
