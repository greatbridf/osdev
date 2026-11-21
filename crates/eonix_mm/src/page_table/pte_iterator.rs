use core::marker::PhantomData;

use super::page_table::PageTableAlloc;
use super::pte::TableAttribute;
use super::{PageTableLevel, PagingMode};
use crate::address::{AddrOps as _, VRange};
use crate::page_table::{PTEntry, RawPageTable};
use crate::paging::{Folio, PageAccess, PFN};

pub struct PageTableIterator<'a, M, A, X>
where
    M: PagingMode,
    A: PageTableAlloc,
    X: PageAccess,
{
    /// Specifies the hierarchy of page table levels to iterate over.
    /// This field determines the sequence of levels in the page table
    /// hierarchy that the iterator will traverse, starting from the
    /// highest level and moving down to the lowest.
    levels: &'static [PageTableLevel],
    remaining: usize,

    indicies: [u16; 8],
    tables: [&'a RawPageTable; 8],

    fill_entry_attr: TableAttribute,

    alloc: A,
    access: X,
    _phantom: PhantomData<M>,
}

impl<'a, M, A, X> PageTableIterator<'a, M, A, X>
where
    M: PagingMode,
    A: PageTableAlloc,
    X: PageAccess,
{
    fn parse_tables_starting_from(&mut self, idx_level: usize) {
        for (idx, &pt_idx) in self
            .indicies
            .iter()
            .enumerate()
            .take(self.levels.len() - 1)
            .skip(idx_level)
        {
            let parent_table = self.tables[idx];
            let next_pte = parent_table[pt_idx].as_table();

            let (mut pfn, attr) = next_pte.get().expect("Not a page table");

            if !attr.contains(TableAttribute::PRESENT) {
                debug_assert_eq!(pfn, PFN::NULL, "Non-present table");
                pfn = self.alloc.alloc().into_raw();
                next_pte.set(pfn, self.fill_entry_attr);
            }

            self.tables[idx + 1] = unsafe {
                // SAFETY: We are creating a pointer to a page referenced to in
                //         some page table, which should be valid.
                let page_table_ptr = self.access.get_ptr_for_pfn(pfn);
                // SAFETY: `page_table_ptr` is a valid pointer to a page table.
                RawPageTable::from_ptr(page_table_ptr)
            };
        }
    }

    pub fn new(
        page_table: &'a RawPageTable, range: VRange,
        fill_entry_attr: TableAttribute, alloc: A, access: X,
    ) -> Self {
        let levels = M::LEVELS;
        let start = range.start().floor();
        let end = range.end().ceil();

        let [.., last_level] = levels else { unreachable!() };

        let mut me = Self {
            levels,
            remaining: (end - start) / last_level.page_size(),
            indicies: [0; 8],
            tables: [const { RawPageTable::null() }; 8],
            fill_entry_attr: fill_entry_attr.union(TableAttribute::PRESENT),
            alloc,
            access,
            _phantom: PhantomData,
        };

        for (i, level) in levels.iter().enumerate() {
            me.indicies[i] = level.index_of(start);
        }

        me.tables[0] = page_table;
        me.parse_tables_starting_from(0);

        me
    }
}

impl<'a, M, A, X> Iterator for PageTableIterator<'a, M, A, X>
where
    M: PagingMode,
    A: PageTableAlloc,
    X: PageAccess,
{
    type Item = &'a PTEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        } else {
            self.remaining -= 1;
        }

        let table_level = self.levels.len() - 1;
        let table = self.tables[table_level];
        let retval = &table[self.indicies[table_level]];

        let idx_level_start_updating = self
            .levels
            .iter()
            .zip(self.indicies.iter_mut())
            .enumerate()
            .rev()
            .skip_while(|(_, (level, idx))| **idx == level.max_index())
            .map(|(i, _)| i)
            .next()
            .expect("Index out of bounds");

        self.indicies[idx_level_start_updating] += 1;
        self.indicies[idx_level_start_updating + 1..=table_level].fill(0);
        self.parse_tables_starting_from(idx_level_start_updating);

        Some(retval)
    }
}
