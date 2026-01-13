use alloc::sync::Arc;

use eonix_mm::paging::PAGE_SIZE;

use crate::kernel::mem::PageCache;

#[derive(Debug, Clone)]
pub struct FileMapping {
    pub page_cache: Arc<PageCache>,
    /// Offset in the file, aligned to 4KB boundary.
    pub offset: usize,
    /// Length of the mapping. Exceeding part will be zeroed.
    pub length: usize,
}

#[derive(Debug, Clone)]
pub enum Mapping {
    // private anonymous memory
    Anonymous,
    // file-backed memory or shared anonymous memory(tmp file)
    File(FileMapping),
}

impl FileMapping {
    pub fn new(page_cache: Arc<PageCache>, offset: usize, length: usize) -> Self {
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
