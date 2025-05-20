use crate::kernel::vfs::dentry::Dentry;
use alloc::sync::Arc;

#[derive(Debug, Clone)]
pub struct FileMapping {
    pub file: Arc<Dentry>,
    /// Offset in the file, aligned to 4KB boundary.
    pub offset: usize,
    /// Length of the mapping. Exceeding part will be zeroed.
    pub length: usize,
}
#[derive(Debug, Clone)]
pub enum Mapping {
    Anonymous,
    File(FileMapping),
}

impl FileMapping {
    pub fn new(file: Arc<Dentry>, offset: usize, length: usize) -> Self {
        assert_eq!(offset & 0xfff, 0);
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
