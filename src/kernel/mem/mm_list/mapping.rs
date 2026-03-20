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

    fn split(&self, _offset: usize) -> (Self, Self) {
        (Self::_new(), Self::_new())
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

    fn split(&self, offset: usize) -> (Self, Self) {
        let (left_len, right_len);

        if offset >= self.length {
            left_len = self.length;
            right_len = 0;
        } else {
            left_len = offset;
            right_len = self.length - offset;
        }

        (
            Self::new(self.page_cache.clone(), self.offset, left_len),
            Self::new(self.page_cache.clone(), self.offset + offset, right_len),
        )
    }
}

impl Mapping {
    pub(super) fn split(&self, offset: usize) -> (Self, Self) {
        match self {
            Mapping::Anonymous(anon_mapping) => {
                let (l, r) = anon_mapping.split(offset);
                (Self::Anonymous(l), Self::Anonymous(r))
            }
            Mapping::File(file_mapping) => {
                let (l, r) = file_mapping.split(offset);
                (Self::File(l), Self::File(r))
            }
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
