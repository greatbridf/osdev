use super::{
    pte::{RawAttribute, TableAttribute},
    PageTableLevel, PagingMode, RawPageTable as _, PTE,
};
use crate::{
    address::{AddrOps as _, VRange},
    paging::{Page, PageAccess, PageAlloc},
};
use core::{marker::PhantomData};

pub struct KernelIterator;
pub struct UserIterator;

pub trait IteratorType<M: PagingMode> {
    fn page_table_attributes() -> TableAttribute;

    fn get_page_table<'a, A, X>(pte: &mut M::Entry, alloc: &A) -> M::RawTable<'a>
    where
        A: PageAlloc,
        X: PageAccess,
    {
        let attr = pte.get_attr().as_table_attr().expect("Not a page table");

        if attr.contains(TableAttribute::PRESENT) {
            let pfn = pte.get_pfn();
            unsafe {
                // SAFETY: We are creating a pointer to a page referenced to in
                //         some page table, which should be valid.
                let page_table_ptr = X::get_ptr_for_pfn(pfn);
                // SAFETY: `page_table_ptr` is a valid pointer to a page table.
                M::RawTable::from_ptr(page_table_ptr)
            }
        } else {
            let page = Page::alloc_in(alloc.clone());
            let page_table_ptr = X::get_ptr_for_page(&page);

            unsafe {
                // SAFETY: `page_table_ptr` is good for writing and properly aligned.
                page_table_ptr.write_bytes(0, 1);
            }

            pte.set(
                page.into_raw(),
                <M::Entry as PTE>::Attr::from(Self::page_table_attributes()),
            );

            unsafe {
                // SAFETY: `page_table_ptr` is a valid pointer to a page table.
                M::RawTable::from_ptr(page_table_ptr)
            }
        }
    }
}

pub struct PageTableIterator<'a, M, A, X, K>
where
    M: PagingMode,
    M::Entry: 'a,
    A: PageAlloc,
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

    alloc: A,
    _phantom: PhantomData<&'a (X, K)>,
}

impl<'a, M, A, X, K> PageTableIterator<'a, M, A, X, K>
where
    M: PagingMode,
    M::Entry: 'a,
    A: PageAlloc,
    X: PageAccess,
    K: IteratorType<M>,
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
            child_table.replace(K::get_page_table::<A, X>(next_pte, &self.alloc));
        }
    }

    pub fn new(page_table: M::RawTable<'a>, range: VRange, alloc: A) -> Self {
        Self::with_levels(page_table, range, alloc, M::LEVELS)
    }

    pub fn with_levels(
        page_table: M::RawTable<'a>,
        range: VRange,
        alloc: A,
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
            alloc,
            _phantom: PhantomData,
        };

        for (i, level) in levels.iter().enumerate() {
            me.indicies[i] = level.index_of(start);
        }

        me.tables[0] = Some(page_table);
        me.parse_tables_starting_from(0);

        me
    }
}

impl<'a, M, A, X, K> Iterator for PageTableIterator<'a, M, A, X, K>
where
    M: PagingMode,
    M::Entry: 'a,
    A: PageAlloc,
    X: PageAccess,
    K: IteratorType<M>,
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

impl<M: PagingMode> IteratorType<M> for KernelIterator {
    fn page_table_attributes() -> TableAttribute {
        TableAttribute::PRESENT | TableAttribute::GLOBAL
    }
}

impl<M: PagingMode> IteratorType<M> for UserIterator {
    fn page_table_attributes() -> TableAttribute {
        TableAttribute::PRESENT | TableAttribute::USER
    }
}
