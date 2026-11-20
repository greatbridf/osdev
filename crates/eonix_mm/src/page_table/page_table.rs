use core::marker::PhantomData;
use core::ops::{Deref, Index};
use core::ptr::NonNull;

use super::paging_mode::PageTableLevel;
use super::pte::TableAttribute;
use super::PagingMode;
use crate::address::{PAddr, VRange};
use crate::page_table::pte::Leaf;
use crate::page_table::{PTEntry, PageTableIterator};
use crate::paging::{Folio, PageAccess, PageBlock, PAGE_SIZE, PFN};

const PTES_PER_PAGE: usize = PAGE_SIZE / size_of::<PTEntry>();

pub trait PageTableAlloc: Clone {
    type Folio: Folio;

    fn alloc(&self) -> Self::Folio;
    unsafe fn from_raw(&self, pfn: PFN) -> Self::Folio;
}

#[repr(transparent)]
pub struct RawTableBase<T: ?Sized> {
    entries: T,
}

pub type RawPageTableSized<const N: usize = PTES_PER_PAGE> =
    RawTableBase<[PTEntry; N]>;
pub type RawPageTable = RawTableBase<[PTEntry]>;

pub struct PageTable<M, A, X>
where
    M: PagingMode,
    A: PageTableAlloc,
    X: PageAccess,
{
    root_table_page: A::Folio,
    alloc: A,
    access: X,
    phantom: PhantomData<M>,
}

impl<M, A, X> PageTable<M, A, X>
where
    M: PagingMode,
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

    pub fn addr(&self) -> PAddr {
        self.root_table_page.start()
    }

    pub fn iter_user(
        &self, range: VRange,
    ) -> impl Iterator<Item = &PTEntry<Leaf>> {
        let page_table_ptr =
            self.access.get_ptr_for_page(&self.root_table_page);
        let root_page_table = unsafe {
            // SAFETY: `page_table_ptr` is a valid pointer to a page table.
            RawPageTable::from_ptr(page_table_ptr)
        };

        PageTableIterator::<M, _, _>::new(
            root_page_table,
            range,
            TableAttribute::USER,
            self.alloc.clone(),
            self.access.clone(),
        )
        .map(PTEntry::as_leaf)
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
    pub fn iter_kernel(
        &self, range: VRange,
    ) -> impl Iterator<Item = &PTEntry<Leaf>> {
        let page_table_ptr =
            self.access.get_ptr_for_page(&self.root_table_page);
        let root_page_table = unsafe {
            // SAFETY: `page_table_ptr` is a valid pointer to a page table.
            RawPageTable::from_ptr(page_table_ptr)
        };

        PageTableIterator::<M, _, _>::with_levels(
            root_page_table,
            range,
            TableAttribute::GLOBAL,
            self.alloc.clone(),
            self.access.clone(),
            M::LEVELS,
        )
        .map(PTEntry::as_leaf)
    }

    fn drop_page_table_recursive(
        &self, page_table: &A::Folio, levels: &[PageTableLevel],
    ) {
        let [level, remaining_levels @ ..] = levels else { return };
        if remaining_levels.is_empty() {
            // We reached the last level, no need to go deeper.
            return;
        }

        let page_table_ptr = self.access.get_ptr_for_page(page_table);
        let page_table = unsafe {
            // SAFETY: `page_table_ptr` is a valid pointer to a page table.
            RawPageTable::from_ptr(page_table_ptr)
        };

        for pte in (0..=level.max_index()).map(|i| page_table.index(i)) {
            let ent = pte.as_table().get();
            pte.clear();

            let Some((pfn, attr)) = ent else { continue };

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

impl<M, A, X> Drop for PageTable<M, A, X>
where
    M: PagingMode,
    A: PageTableAlloc,
    X: PageAccess,
{
    fn drop(&mut self) {
        self.drop_page_table_recursive(&self.root_table_page, M::LEVELS);
    }
}

impl RawPageTable {
    pub const fn null() -> &'static Self {
        &RawPageTableSized { entries: [] }
    }

    pub unsafe fn from_ptr<'a>(ptr: NonNull<PageBlock>) -> &'a Self {
        // XXX: Consider non-default sized page tables.

        unsafe {
            // SAFETY: The caller guarantees that deferencing ptr is fine.
            ptr.cast::<RawPageTableSized>().as_ref() as &Self
        }
    }
}

impl Index<u16> for RawPageTable {
    type Output = PTEntry;

    fn index(&self, index: u16) -> &Self::Output {
        &self.entries[index as usize]
    }
}

impl<const N: usize> Deref for RawPageTableSized<N> {
    type Target = RawPageTable;

    fn deref(&self) -> &Self::Target {
        self as &RawPageTable
    }
}

impl Deref for RawPageTable {
    type Target = [PTEntry];

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}
