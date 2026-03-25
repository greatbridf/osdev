use alloc::sync::Arc;

use eonix_mm::paging::{Folio as _, PFN};

use crate::kernel::mem::{Folio, FolioOwned, PageCache, PageOffset};

#[derive(Debug, Clone)]
pub struct FileMapping {
    pub page_cache: Arc<PageCache>,
    /// Offset in the file, aligned to 4KB boundary.
    pub offset: PageOffset,
    /// Length of the mapping. Exceeding part will be zeroed.
    pub length: usize,
}

#[derive(Debug, Clone)]
pub struct AnonMapping();

#[derive(Debug, Clone)]
pub enum Mapping {
    /// Anonymous mappings that reside in memory only.
    ///
    /// All anonymous mappings are private. If shared mappings are needed, use
    /// tmpfs together with shared file mappings.
    Anonymous(AnonMapping),
    /// File backed mappings that is copy on write across duplications and not
    /// written back to the underlying [Inode].
    ///
    /// [Inode]: crate::kernel::vfs::inode::Inode
    PrivateFile {
        anon: AnonMapping,
        file: FileMapping,
    },
    /// Shared file mappings.
    SharedFile(FileMapping),
}

impl AnonMapping {
    const fn new() -> Self {
        Self()
    }

    fn split(&self, _offset: PageOffset) -> (Self, Self) {
        (Self::new(), Self::new())
    }

    pub fn add_folio(&self, folio: FolioOwned) -> Folio {
        folio.share()
    }
}

impl FileMapping {
    fn new(
        page_cache: Arc<PageCache>, offset: PageOffset, length: usize,
    ) -> Self {
        Self {
            page_cache,
            offset,
            length,
        }
    }

    fn split(&self, offset: PageOffset) -> (Self, Self) {
        let (left_len, right_len);

        if offset.byte_count() >= self.length {
            left_len = self.length;
            right_len = 0;
        } else {
            left_len = offset.byte_count();
            right_len = self.length - offset.byte_count();
        }

        (
            Self::new(self.page_cache.clone(), self.offset, left_len),
            Self::new(self.page_cache.clone(), self.offset + offset, right_len),
        )
    }
}

impl Mapping {
    pub fn new_anon() -> Self {
        Self::Anonymous(AnonMapping::new())
    }

    pub fn new_file_priv(
        page_cache: Arc<PageCache>, offset: usize, length: usize,
    ) -> Self {
        Self::PrivateFile {
            file: FileMapping::new(
                page_cache,
                PageOffset::from_byte_aligned(offset),
                length,
            ),
            anon: AnonMapping::new(),
        }
    }

    pub(super) fn split(&self, offset: usize) -> (Self, Self) {
        let offset = PageOffset::from_byte_aligned(offset);

        match self {
            Mapping::Anonymous(anon_mapping) => {
                let (l, r) = anon_mapping.split(offset);
                (Self::Anonymous(l), Self::Anonymous(r))
            }
            Mapping::PrivateFile { file, anon } => {
                let (l_file, r_file) = file.split(offset);
                let (l_anon, r_anon) = anon.split(offset);

                (
                    Self::PrivateFile {
                        file: l_file,
                        anon: l_anon,
                    },
                    Self::PrivateFile {
                        file: r_file,
                        anon: r_anon,
                    },
                )
            }
            Mapping::SharedFile(file_mapping) => {
                let (l, r) = file_mapping.split(offset);

                (Self::SharedFile(l), Self::SharedFile(r))
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
