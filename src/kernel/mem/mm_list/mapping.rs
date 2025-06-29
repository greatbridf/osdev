use core::fmt::Debug;

use crate::kernel::vfs::inode::Inode;
use alloc::sync::Arc;
use eonix_mm::paging::PAGE_SIZE;

#[derive(Debug, Clone)]
pub struct FileMapping {
    pub file: Arc<dyn Inode>,
    /// Offset in the file, aligned to 4KB boundary.
    pub offset: usize,
    /// Length of the mapping. Exceeding part will be zeroed.
    pub length: usize,
}

impl Debug for dyn Inode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Inode()")
    }
}

#[derive(Debug, Clone)]
pub enum Mapping {
    // private anonymous memory
    Anonymous,
    // file-backed memory or shared anonymous memory(tmp file)
    File(FileMapping),
}

impl FileMapping {
    pub fn new(file: Arc<dyn Inode>, offset: usize, length: usize) -> Self {
        assert_eq!(offset & (PAGE_SIZE - 1), 0);
        Self {
            file,
            offset,
            length,
        }
    }

    pub fn offset(&self, offset: usize) -> Self {
        if self.length <= offset {
            Self::new(self.file.clone(), self.offset + self.length, 0)
        } else {
            Self::new(
                self.file.clone(),
                self.offset + offset,
                self.length - offset,
            )
        }
    }
}
