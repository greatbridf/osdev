use core::marker::PhantomData;

use streaming_iterator::StreamingIterator;

use super::page_table::PageTableAlloc;
use super::pte::TableAttribute;
use super::{PageTableLevel, PagingMode};
use crate::address::{AddrOps as _, VAddr, VRange};
use crate::page_table::{PTEntry, RawPageTable};
use crate::paging::{Folio, PageAccess, PFN};

const MAX_LEVELS: usize = 5;

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
    next_addr: VAddr,
    end_addr: VAddr,

    tables: [&'a RawPageTable; MAX_LEVELS],
    cur_entry: Option<&'a PTEntry>,

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
    pub fn new(
        page_table: &'a RawPageTable, range: VRange,
        fill_entry_attr: TableAttribute, alloc: A, access: X,
    ) -> Self {
        const { assert!(M::LEVELS.len() <= MAX_LEVELS) }

        Self {
            levels: M::LEVELS,
            next_addr: range.start().floor(),
            end_addr: range.end().ceil(),
            tables: {
                let mut tables = [const { RawPageTable::null() }; MAX_LEVELS];
                tables[0] = page_table;
                tables
            },
            cur_entry: None,
            fill_entry_attr: fill_entry_attr.union(TableAttribute::PRESENT),
            alloc,
            access,
            _phantom: PhantomData,
        }
    }

    fn parse_table(&mut self, level_idx: usize, pt_idx: u16) {
        let parent_table = self.tables[level_idx - 1];
        let next_pte = parent_table[pt_idx].as_table();

        let (mut pfn, attr) = next_pte.get().expect("Not a page table");

        if !attr.contains(TableAttribute::PRESENT) {
            debug_assert_eq!(pfn, PFN::NULL, "Non-present table");
            pfn = self.alloc.alloc().into_raw();
            next_pte.set(pfn, self.fill_entry_attr);
        }

        self.tables[level_idx] = unsafe {
            // SAFETY: We are creating a pointer to a page referenced to in
            //         some page table, which should be valid.
            let page_table_ptr = self.access.get_ptr_for_pfn(pfn);
            // SAFETY: `page_table_ptr` is a valid pointer to a page table.
            RawPageTable::from_ptr(page_table_ptr)
        };

        debug_assert!(!self.tables[level_idx].is_empty());
    }

    pub fn next_pte(&mut self) {
        if self.next_addr >= self.end_addr {
            self.cur_entry = None;
            return;
        }

        let levels_len = self.levels.len();
        let last_level = self.levels[levels_len - 1];

        // Level 0 (root table) is always ready
        let mut idx_ready = 0;
        let mut indicies = [0; MAX_LEVELS];

        for (level, idx) in self.levels.iter().zip(indicies.iter_mut()) {
            *idx = level.index_of(self.next_addr);
        }

        for i in 1..levels_len {
            if self.tables[i].is_empty() {
                self.parse_table(i, indicies[i - 1]);
                idx_ready = i;
            } else if indicies[i] != 0 {
                idx_ready = i;
            }
        }

        for i in (idx_ready + 1)..levels_len {
            self.parse_table(i, indicies[i - 1]);
        }

        let last_table = self.tables[levels_len - 1];
        let last_idx = indicies[levels_len - 1];

        self.cur_entry = Some(&last_table[last_idx]);
        self.next_addr = self.next_addr + last_level.page_size();
    }
}

impl<M, A, X> StreamingIterator for PageTableIterator<'_, M, A, X>
where
    M: PagingMode,
    A: PageTableAlloc,
    X: PageAccess,
{
    type Item = PTEntry;

    fn advance(&mut self) {
        self.next_pte();
    }

    fn get(&self) -> Option<&Self::Item> {
        self.cur_entry
    }
}
