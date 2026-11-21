use core::marker::PhantomData;
use core::ops::{Deref, Index};
use core::ptr::NonNull;

use streaming_iterator::StreamingIterator;

use super::paging_mode::PageTableLevel;
use super::pte::TableAttribute;
use super::PagingMode;
use crate::address::{PAddr, VRange};
use crate::page_table::pte::Leaf;
use crate::page_table::{PTEntry, PageTableIterator};
use crate::paging::{Folio, PageAccess, PageBlock, PAGE_SIZE, PFN};

const PTES_PER_PAGE: usize = PAGE_SIZE / size_of::<PTEntry>();

pub trait TableManage {
    type Table: Folio;
    type Mapped: Deref<Target = RawPageTable>;

    const LEVELS: &'static [PageTableLevel];

    fn alloc(&self) -> Self::Table;
    fn map(&self, folio: &Self::Table) -> Self::Mapped;
    unsafe fn from_raw(&self, pfn: PFN) -> Self::Table;
}

pub trait PageTableAlloc: Clone {
    type Folio: Folio;

    fn alloc(&self) -> Self::Folio;
    unsafe fn from_raw(&self, pfn: PFN) -> Self::Folio;
}

pub struct TableManager<T: PageTableAlloc, U: PageAccess, M: PagingMode> {
    pub alloc: T,
    pub mapper: U,
    pub mode: PhantomData<M>,
}

#[repr(transparent)]
pub struct RawTableBase<T: ?Sized> {
    entries: T,
}

pub type RawPageTableSized<const N: usize = PTES_PER_PAGE> =
    RawTableBase<[PTEntry; N]>;
pub type RawPageTable = RawTableBase<[PTEntry]>;

pub struct PageTable<T: TableManage> {
    root_table: T::Table,
    manager: T,
}

impl<T: TableManage> PageTable<T> {
    pub fn new(root_table: T::Table, manager: T) -> Self {
        Self {
            root_table,
            manager,
        }
    }

    pub fn addr(&self) -> PAddr {
        self.root_table.start()
    }

    pub fn iter_user(
        &self, range: VRange,
    ) -> impl StreamingIterator<Item = PTEntry<Leaf>> + use<'_, T> {
        let root_table = self.manager.map(&self.root_table);

        PageTableIterator::new(
            root_table,
            range,
            TableAttribute::USER,
            &self.manager,
        )
        .map_ref(PTEntry::as_leaf)
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
    ) -> impl StreamingIterator<Item = PTEntry<Leaf>> + use<'_, T> {
        let root_table = self.manager.map(&self.root_table);

        PageTableIterator::new(
            root_table,
            range,
            TableAttribute::GLOBAL,
            &self.manager,
        )
        .map_ref(PTEntry::as_leaf)
    }

    fn drop_page_table_recursive(
        &self, table: &RawPageTable, levels: &[PageTableLevel],
    ) {
        let [level, remaining_levels @ ..] = levels else { return };
        if remaining_levels.is_empty() {
            // We reached the last level, no need to go deeper.
            return;
        }

        for pte in table.iter().take(1 + level.max_index() as usize) {
            let ent = pte.as_table().get();
            pte.clear();

            let Some((pfn, attr)) = ent else { continue };

            if !attr.contains(TableAttribute::PRESENT | TableAttribute::USER) {
                continue;
            }

            let table = unsafe {
                // SAFETY: We got the pfn from a valid page table entry, so it should be valid.
                self.manager.from_raw(pfn)
            };

            self.drop_page_table_recursive(
                &self.manager.map(&table),
                remaining_levels,
            );
        }
    }
}

impl<T: TableManage> Drop for PageTable<T> {
    fn drop(&mut self) {
        let table = self.manager.map(&self.root_table);
        self.drop_page_table_recursive(&table, T::LEVELS);
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

impl<T, U, M> TableManage for TableManager<T, U, M>
where
    T: PageTableAlloc,
    T::Folio: Folio,
    U: PageAccess,
    M: PagingMode,
{
    type Table = T::Folio;
    type Mapped = &'static RawPageTable;

    const LEVELS: &'static [PageTableLevel] = M::LEVELS;

    fn alloc(&self) -> Self::Table {
        self.alloc.alloc()
    }

    fn map(&self, folio: &Self::Table) -> Self::Mapped {
        unsafe { RawPageTable::from_ptr(self.mapper.get_ptr_for_page(folio)) }
    }

    unsafe fn from_raw(&self, pfn: PFN) -> Self::Table {
        unsafe { self.alloc.from_raw(pfn) }
    }
}
