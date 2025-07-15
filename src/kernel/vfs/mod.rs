use crate::kernel::constants::{S_IFBLK, S_IFCHR, S_IFDIR, S_IFLNK, S_IFMT, S_IFREG};
use crate::prelude::*;
use alloc::sync::Arc;
use dentry::Dentry;
use eonix_sync::LazyLock;
use inode::Mode;

pub mod dentry;
pub mod file;
pub mod filearray;
pub mod inode;
pub mod mount;
pub mod vfs;

pub type DevId = u32;

pub fn s_isreg(mode: Mode) -> bool {
    (mode & S_IFMT) == S_IFREG
}

pub fn s_isdir(mode: Mode) -> bool {
    (mode & S_IFMT) == S_IFDIR
}

pub fn s_ischr(mode: Mode) -> bool {
    (mode & S_IFMT) == S_IFCHR
}

pub fn s_isblk(mode: Mode) -> bool {
    (mode & S_IFMT) == S_IFBLK
}

pub fn s_islnk(mode: Mode) -> bool {
    (mode & S_IFMT) == S_IFLNK
}

pub struct FsContext {
    pub fsroot: Arc<Dentry>,
    pub cwd: Spin<Arc<Dentry>>,
    pub umask: Spin<Mode>,
}

static GLOBAL_FS_CONTEXT: LazyLock<Arc<FsContext>> = LazyLock::new(|| {
    Arc::new(FsContext {
        fsroot: Dentry::root().clone(),
        cwd: Spin::new(Dentry::root().clone()),
        umask: Spin::new(0o022),
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
