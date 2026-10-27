use core::marker::PhantomData;
use core::ptr::NonNull;

use super::paging_mode::PageTableLevel;
use super::pte::{RawAttribute, TableAttribute};
use super::{PagingMode, PTE};
use crate::address::{PAddr, VRange};
use crate::page_table::PageTableIterator;
use crate::paging::{Folio, PageAccess, PageBlock, PFN};

pub trait RawPageTable<'a>: Send + 'a {
    type Entry: PTE + 'a;

    /// Return the entry at the given index.
    fn index(&self, index: u16) -> &'a Self::Entry;

    /// Return a mutable reference to the entry at the given index.
    fn index_mut(&mut self, index: u16) -> &'a mut Self::Entry;

    /// Get the page table pointed to by raw pointer `ptr`.
    unsafe fn from_ptr(ptr: NonNull<PageBlock>) -> Self;
}

pub trait PageTableAlloc: Clone {
    type Folio: Folio;

    fn alloc(&self) -> Self::Folio;
    unsafe fn from_raw(&self, pfn: PFN) -> Self::Folio;
}

pub trait GlobalPageTableAlloc: PageTableAlloc {
    const GLOBAL: Self;
}

pub struct PageTable<'a, M, A, X>
where
    M: PagingMode,
    M::Entry: 'a,
    A: PageTableAlloc,
    X: PageAccess,
{
    root_table_page: A::Folio,
    alloc: A,
    access: X,
    phantom: PhantomData<&'a M>,
}

impl<'a, M, A, X> PageTable<'a, M, A, X>
where
    M: PagingMode,
    M::Entry: 'a,
    A: PageTableAlloc,
    X: PageAccess,
{
    pub fn new(root_table_page: A::Folio, alloc: A, access: X) -> Self {
        Self {
            root_table_page,
            alloc,
            access,
            phantom: PhantomData,
        }
    }

    pub fn clone_global<'b, B>(&self) -> PageTable<'b, M, B, X>
    where
        B: GlobalPageTableAlloc,
    {
        self.clone_in(B::GLOBAL)
    }

    pub fn clone_in<'b, B>(&self, alloc: B) -> PageTable<'b, M, B, X>
    where
        B: PageTableAlloc,
    {
        let new_root_table_page = alloc.alloc();
        let new_table_data = self.access.get_ptr_for_page(&new_root_table_page);
        let kernel_table_data = self.access.get_ptr_for_page(&self.root_table_page);

        unsafe {
            // SAFETY: `new_table_data` and `kernel_table_data` are both valid pointers
            //         to **different** page tables.
            new_table_data.copy_from_nonoverlapping(kernel_table_data, 1);
        }

        let mut root_page_table = unsafe {
            // SAFETY: `page_table_ptr` is a valid pointer to a page table.
            M::RawTable::from_ptr(new_table_data)
        };

        let level0 = M::LEVELS[0];
        for idx in 0..=level0.max_index() / 2 {
            // We consider the first half of the page table as user space.
            // Clear all (potential) user space mappings.
            root_page_table.index_mut(idx).take();
        }

        PageTable::new(new_root_table_page, alloc, self.access.clone())
    }

    pub fn addr(&self) -> PAddr {
        self.root_table_page.start()
    }

    pub fn iter_user(&self, range: VRange) -> impl Iterator<Item = &mut M::Entry> {
        let page_table_ptr = self.access.get_ptr_for_page(&self.root_table_page);
        let root_page_table = unsafe {
            // SAFETY: `page_table_ptr` is a valid pointer to a page table.
            M::RawTable::from_ptr(page_table_ptr)
        };

        PageTableIterator::<M, _, _>::new(
            root_page_table,
            range,
            TableAttribute::USER,
            self.alloc.clone(),
            self.access.clone(),
        )
    }

    /// Iterates over the kernel space entries in the page table.
    ///
    /// # Returns
    /// An iterator over mutable references to the page table entries (`M::Entry`) within the
    /// specified range.
    ///
    /// # Example
    /// ```
    /// let range = VRange::new(0x1234000, 0x1300000);
    /// for pte in page_table.iter_kernel(range) {
    ///     // Process each entry
    /// }
    /// ```
    pub fn iter_kernel(&self, range: VRange) -> impl Iterator<Item = &mut M::Entry> {
        let page_table_ptr = self.access.get_ptr_for_page(&self.root_table_page);
        let root_page_table = unsafe {
            // SAFETY: `page_table_ptr` is a valid pointer to a page table.
            M::RawTable::from_ptr(page_table_ptr)
        };

        PageTableIterator::<M, _, _>::with_levels(
            root_page_table,
            range,
            TableAttribute::GLOBAL,
            self.alloc.clone(),
            self.access.clone(),
            M::LEVELS,
        )
    }

    fn drop_page_table_recursive(&self, page_table: &A::Folio, levels: &[PageTableLevel]) {
        let [level, remaining_levels @ ..] = levels else { return };
        if remaining_levels.is_empty() {
            // We reached the last level, no need to go deeper.
            return;
        }

        let page_table_ptr = self.access.get_ptr_for_page(page_table);
        let mut page_table = unsafe {
            // SAFETY: `page_table_ptr` is a valid pointer to a page table.
            M::RawTable::from_ptr(page_table_ptr)
        };

        for pte in (0..=level.max_index()).map(|i| page_table.index_mut(i)) {
            let (pfn, attr) = pte.take();
            let Some(attr) = attr.as_table_attr() else {
                continue;
            };

            if !attr.contains(TableAttribute::PRESENT | TableAttribute::USER) {
                continue;
            }

            let page_table = unsafe {
                // SAFETY: We got the pfn from a valid page table entry, so it should be valid.
                self.alloc.from_raw(pfn)
            };

            self.drop_page_table_recursive(&page_table, remaining_levels);
        }
    }
}

impl<'a, M, A, X> Drop for PageTable<'a, M, A, X>
where
    M: PagingMode,
    M::Entry: 'a,
    A: PageTableAlloc,
    X: PageAccess,
{
    fn drop(&mut self) {
        self.drop_page_table_recursive(&self.root_table_page, M::LEVELS);
    }
}
