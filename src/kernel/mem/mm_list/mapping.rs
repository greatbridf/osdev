use alloc::sync::Arc;

use eonix_mm::paging::{Folio as _, PAGE_SIZE, PFN};

use crate::kernel::mem::{Folio, PageCache};

#[derive(Debug, Clone)]
pub struct FileMapping {
    pub page_cache: Arc<PageCache>,
    /// Offset in the file, aligned to 4KB boundary.
    pub offset: usize,
    /// Length of the mapping. Exceeding part will be zeroed.
    pub length: usize,
}

#[derive(Debug, Clone)]
pub struct AnonMapping();

#[derive(Debug, Clone)]
pub enum Mapping {
    // private anonymous memory
    Anonymous(AnonMapping),
    // file-backed memory or shared anonymous memory(tmp file)
    File(FileMapping),
}

impl AnonMapping {
    const fn _new() -> Self {
        Self()
    }

    pub const fn new() -> Mapping {
        Mapping::Anonymous(Self::_new())
    }
}

impl FileMapping {
    pub fn new(
        page_cache: Arc<PageCache>, offset: usize, length: usize,
    ) -> Self {
        assert_eq!(offset & (PAGE_SIZE - 1), 0);
        Self {
            page_cache,
            offset,
            length,
        }
    }

    pub fn offset(&self, offset: usize) -> Self {
        if self.length <= offset {
            Self::new(self.page_cache.clone(), self.offset + self.length, 0)
        } else {
            Self::new(
                self.page_cache.clone(),
                self.offset + offset,
                self.length - offset,
            )
        }
    }
}

/// Turn the folio together with its refcount into a raw [`PFN`] that can be
/// mapped into some page table.
pub fn add_mapping(folio: Folio) -> PFN {
    folio.into_raw()
}

/// Duplicate the mapping in page tables.
///
/// # Safety
/// `pfn` must be previously created via [`add_mapping`].
///
/// Otherwise this is undefined behavior.
pub unsafe fn duplicate_mapping(pfn: PFN) -> PFN {
    unsafe {
        // SAFETY: `pfn` is created via `add_mapping`, which uses `into_raw`.
        Folio::with_raw(pfn, |folio| add_mapping(folio.clone()))
    }
}

/// Remove mappings in page tables.
///
/// # Safety
/// `pfn` must be previously created via [`add_mapping`].
///
/// Otherwise this is undefined behavior.
pub unsafe fn remove_mapping(pfn: PFN) -> Folio {
    unsafe {
        // SAFETY: `pfn` is created via `add_mapping`, which uses `into_raw`.
        Folio::from_raw(pfn)
    }
}
