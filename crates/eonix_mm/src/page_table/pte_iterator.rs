use super::page_table::PageTableAlloc;
use super::pte::{RawAttribute, TableAttribute};
use super::{PageTableLevel, PagingMode, RawPageTable as _, PTE};
use crate::address::{AddrOps as _, VRange};
use crate::paging::{Folio, PageAccess};

pub struct PageTableIterator<'a, M, A, X>
where
    M: PagingMode,
    M::Entry: 'a,
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
    tables: [Option<M::RawTable<'a>>; 8],

    fill_entry_attr: TableAttribute,

    alloc: A,
    access: X,
}

impl<'a, M, A, X> PageTableIterator<'a, M, A, X>
where
    M: PagingMode,
    M::Entry: 'a,
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
            let [parent_table, child_table] = unsafe {
                // SAFETY: `idx` and `idx + 1` must not overlap.
                //         `idx + 1` is always less than `levels_len` since we iterate
                //         until `levels_len - 1`.
                self.tables.get_disjoint_unchecked_mut([idx, idx + 1])
            };
            let parent_table = parent_table.as_mut().expect("Parent table is None");
            let next_pte = parent_table.index_mut(pt_idx);

            child_table.replace({
                let attr = next_pte
                    .get_attr()
                    .as_table_attr()
                    .expect("Not a page table");

                if attr.contains(TableAttribute::PRESENT) {
                    let pfn = next_pte.get_pfn();
                    unsafe {
                        // SAFETY: We are creating a pointer to a page referenced to in
                        //         some page table, which should be valid.
                        let page_table_ptr = self.access.get_ptr_for_pfn(pfn);
                        // SAFETY: `page_table_ptr` is a valid pointer to a page table.
                        M::RawTable::from_ptr(page_table_ptr)
                    }
                } else {
                    let page = self.alloc.alloc();
                    let page_table_ptr = self.access.get_ptr_for_page(&page);

                    unsafe {
                        // SAFETY: `page_table_ptr` is good for writing and properly aligned.
                        page_table_ptr.write_bytes(0, 1);
                    }

                    next_pte.set(page.into_raw(), self.fill_entry_attr.into());

                    unsafe {
                        // SAFETY: `page_table_ptr` is a valid pointer to a page table.
                        M::RawTable::from_ptr(page_table_ptr)
                    }
                }
            });
        }
    }

    pub fn new(
        page_table: M::RawTable<'a>,
        range: VRange,
        fill_entry_attr: TableAttribute,
        alloc: A,
        access: X,
    ) -> Self {
        Self::with_levels(page_table, range, fill_entry_attr, alloc, access, M::LEVELS)
    }

    pub fn with_levels(
        page_table: M::RawTable<'a>,
        range: VRange,
        fill_entry_attr: TableAttribute,
        alloc: A,
        access: X,
        levels: &'static [PageTableLevel],
    ) -> Self {
        let start = range.start().floor();
        let end = range.end().ceil();

        let [.., last_level] = levels else { unreachable!() };

        let mut me = Self {
            levels,
            remaining: (end - start) / last_level.page_size(),
            indicies: [0; 8],
            tables: [const { None }; 8],
            fill_entry_attr: fill_entry_attr.union(TableAttribute::PRESENT),
            alloc,
            access,
        };

        for (i, level) in levels.iter().enumerate() {
            me.indicies[i] = level.index_of(start);
        }

        me.tables[0] = Some(page_table);
        me.parse_tables_starting_from(0);

        me
    }
}

impl<'a, M, A, X> Iterator for PageTableIterator<'a, M, A, X>
where
    M: PagingMode,
    M::Entry: 'a,
    A: PageTableAlloc,
    X: PageAccess,
{
    type Item = &'a mut M::Entry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        } else {
            self.remaining -= 1;
        }

        let table_level = self.levels.len() - 1;
        let retval = self.tables[table_level]
            .as_mut()
            .unwrap()
            .index_mut(self.indicies[table_level]);

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
