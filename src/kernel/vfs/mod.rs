use crate::prelude::*;

use alloc::sync::Arc;
use bindings::{dev_t, S_IFBLK, S_IFCHR, S_IFDIR, S_IFLNK, S_IFMT, S_IFREG};
use dentry::Dentry;
use inode::Mode;

use super::task::Thread;

pub mod dentry;
pub mod file;
pub mod filearray;
pub mod inode;
pub mod mount;
pub mod vfs;

pub type DevId = dev_t;

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

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct TimeSpec {
    pub sec: u64,
    pub nsec: u64,
}

#[derive(Clone)]
pub struct FsContext {
    pub fsroot: Arc<Dentry>,
    pub cwd: Spin<Arc<Dentry>>,
    pub umask: Spin<Mode>,
}

impl FsContext {
    pub fn get_current<'lt>() -> &'lt Arc<Self> {
        let current = Thread::current();
        &current.fs_context
    }

    pub fn new_for_init() -> Arc<Self> {
        Arc::new(FsContext {
            fsroot: Dentry::kernel_root_dentry(),
            cwd: Spin::new(Dentry::kernel_root_dentry()),
            umask: Spin::new(0o022),
        })
    }

    pub fn new_cloned(other: &Self) -> Arc<Self> {
        Arc::new(Self {
            fsroot: other.fsroot.clone(),
            cwd: other.cwd.clone(),
            umask: other.umask.clone(),
        })
    }

    pub fn new_shared(other: &Arc<Self>) -> Arc<Self> {
        other.clone()
    }
}
