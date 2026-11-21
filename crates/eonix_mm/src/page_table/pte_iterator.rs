use core::ops::Deref;

use streaming_iterator::StreamingIterator;

use super::pte::TableAttribute;
use super::PageTableLevel;
use crate::address::{AddrOps as _, VAddr, VRange};
use crate::page_table::page_table::{TableManage, TableMapped};
use crate::page_table::{PTEntry, RawPageTable};
use crate::paging::{Folio, PFN};

const MAX_LEVELS: usize = 5;

enum TableRef<T: TableMapped> {
    Mapped(T),
    Blank,
}

impl<T: TableMapped> Deref for TableRef<T> {
    type Target = RawPageTable;

    fn deref(&self) -> &Self::Target {
        match self {
            TableRef::Mapped(mapped) => mapped,
            TableRef::Blank => RawPageTable::null(),
        }
    }
}

pub struct PageTableIterator<'a, T>
where
    T: TableManage,
{
    /// Specifies the hierarchy of page table levels to iterate over.
    /// This field determines the sequence of levels in the page table
    /// hierarchy that the iterator will traverse, starting from the
    /// highest level and moving down to the lowest.
    levels: &'static [PageTableLevel],
    next_addr: VAddr,
    end_addr: VAddr,

    tables: [TableRef<T::Mapped>; MAX_LEVELS],
    indicies: [u16; MAX_LEVELS],
    done: bool,

    fill_entry_attr: TableAttribute,
    manager: &'a T,
}

impl<'a, T: TableManage> PageTableIterator<'a, T> {
    pub fn new(
        page_table: T::Mapped, range: VRange, fill_entry_attr: TableAttribute,
        manager: &'a T,
    ) -> Self {
        const { assert!(T::LEVELS.len() <= MAX_LEVELS) }

        Self {
            levels: T::LEVELS,
            next_addr: range.start().floor(),
            end_addr: range.end().ceil(),
            tables: {
                let mut tables = [const { TableRef::Blank }; MAX_LEVELS];
                tables[0] = TableRef::Mapped(page_table);
                tables
            },
            indicies: [0; MAX_LEVELS],
            done: false,
            fill_entry_attr: fill_entry_attr.union(TableAttribute::PRESENT),
            manager,
        }
    }

    fn get_entry(&self, level_idx: usize) -> &PTEntry {
        let table = &self.tables[level_idx];
        &table[self.indicies[level_idx]]
    }

    fn parse_indicies(&mut self) {
        for (level, idx) in self.levels.iter().zip(self.indicies.iter_mut()) {
            *idx = level.index_of(self.next_addr);
        }
    }

    fn map_to_ref(&self, pfn: PFN) -> TableRef<T::Mapped> {
        TableRef::Mapped({
            let table = unsafe { self.manager.from_raw(pfn) };
            let mapped = self.manager.map(&table);
            table.into_raw();

            mapped
        })
    }

    fn parse_table(&mut self, level_idx: usize) {
        assert!(level_idx > 0);

        let next_pte = self.get_entry(level_idx - 1).as_table();
        let (mut pfn, attr) = next_pte.get().expect("Not a page table");

        if !attr.contains(TableAttribute::PRESENT) {
            debug_assert_eq!(pfn, PFN::NULL, "Non-present table");
            pfn = self.manager.alloc().into_raw();
            next_pte.set(pfn, self.fill_entry_attr);
        }

        self.tables[level_idx] = self.map_to_ref(pfn);

        debug_assert!(!self.tables[level_idx].is_empty());
    }

    fn parse_tables(&mut self, prev_indicies: &[u16; MAX_LEVELS]) {
        let levels_len = self.levels.len();
        let mut idx_update = levels_len;

        for i in 1..levels_len {
            let curr_idx = self.indicies[i - 1];
            let prev_idx = prev_indicies[i - 1];

            if self.tables[i].is_empty() || curr_idx != prev_idx {
                idx_update = i;
                break;
            }
        }

        for i in idx_update..levels_len {
            self.parse_table(i);
        }
    }

    fn test_mark_done(&mut self) -> bool {
        if self.next_addr >= self.end_addr {
            self.done = true;
        }

        self.done
    }

    fn pte_size(&self) -> usize {
        self.levels.last().unwrap().page_size()
    }

    fn step_pte(&mut self) {
        if self.test_mark_done() {
            return;
        }

        let prev_indicies = self.indicies;

        self.parse_indicies();
        self.parse_tables(&prev_indicies);

        self.next_addr = self.next_addr + self.pte_size();
    }
}

impl<T: TableManage> StreamingIterator for PageTableIterator<'_, T> {
    type Item = PTEntry;

    fn advance(&mut self) {
        self.step_pte();
    }

    fn get(&self) -> Option<&Self::Item> {
        if self.done {
            return None;
        }

        Some(self.get_entry(self.levels.len() - 1))
    }
}
