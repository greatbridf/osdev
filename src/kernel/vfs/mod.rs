pub mod dentry;
mod file;
pub mod filearray;
pub mod inode;
pub mod mount;
mod superblock;
pub mod types;

use crate::prelude::*;
use alloc::sync::Arc;
use dentry::Dentry;
use eonix_sync::LazyLock;
use types::Permission;

pub use file::{File, FileType, PollEvent, SeekOption, TerminalFile};
pub use superblock::{SbRef, SbUse, SuperBlock, SuperBlockInfo, SuperBlockLock};

pub struct FsContext {
    pub fsroot: Arc<Dentry>,
    pub cwd: Spin<Arc<Dentry>>,
    pub umask: Spin<Permission>,
}

static GLOBAL_FS_CONTEXT: LazyLock<Arc<FsContext>> = LazyLock::new(|| {
    Arc::new(FsContext {
        fsroot: Dentry::root().clone(),
        cwd: Spin::new(Dentry::root().clone()),
        umask: Spin::new(Permission::new(0o755)),
    })
});

impl FsContext {
    pub fn global() -> &'static Arc<Self> {
        &GLOBAL_FS_CONTEXT
    }

    pub fn new_cloned(other: &Self) -> Arc<Self> {
        Arc::new(Self {
            fsroot: other.fsroot.clone(),
            cwd: Spin::new(other.cwd.lock().clone()),
            umask: Spin::new(other.umask.lock().clone()),
        })
    }

    #[allow(dead_code)]
    pub fn new_shared(other: &Arc<Self>) -> Arc<Self> {
        other.clone()
    }
}
