use crate::prelude::*;
use alloc::sync::Arc;
use dentry::Dentry;
use eonix_sync::LazyLock;
use inode::Mode;

pub mod dentry;
mod file;
pub mod filearray;
pub mod inode;
pub mod mount;
pub mod vfs;

pub use file::{EventFile, File, FileType, PollEvent, SeekOption, TerminalFile};

pub type DevId = u32;

pub struct FsContext {
    pub fsroot: Arc<Dentry>,
    pub cwd: Spin<Arc<Dentry>>,
    pub umask: Spin<Mode>,
}

static GLOBAL_FS_CONTEXT: LazyLock<Arc<FsContext>> = LazyLock::new(|| {
    Arc::new(FsContext {
        fsroot: Dentry::root().clone(),
        cwd: Spin::new(Dentry::root().clone()),
        umask: Spin::new(Mode::new(0o022)),
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
