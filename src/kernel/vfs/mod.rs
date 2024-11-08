use crate::prelude::*;

use alloc::sync::Arc;
use bindings::{current_process, dev_t, S_IFBLK, S_IFCHR, S_IFDIR, S_IFLNK, S_IFMT, S_IFREG};
use dentry::Dentry;
use inode::Mode;

pub mod dentry;
pub mod ffi;
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
    pub fn get_current() -> BorrowedArc<'static, Self> {
        // SAFETY: There should always be a current process.
        let current = unsafe { current_process.as_ref().unwrap() };
        let ptr = current.fs_context.m_handle as *const _ as *const Self;

        BorrowedArc::from_raw(ptr)
    }
}

#[no_mangle]
pub extern "C" fn r_fs_context_drop(other: *const FsContext) {
    // SAFETY: `other` is a valid pointer from `Arc::into_raw()`.
    unsafe { Arc::from_raw(other) };
}

#[no_mangle]
pub extern "C" fn r_fs_context_new_cloned(other: *const FsContext) -> *const FsContext {
    // SAFETY: `other` is a valid pointer from `Arc::into_raw()`.
    let other = BorrowedArc::from_raw(other);

    Arc::into_raw(Arc::new(FsContext {
        fsroot: other.fsroot.clone(),
        cwd: other.cwd.clone(),
        umask: other.umask.clone(),
    }))
}

#[no_mangle]
pub extern "C" fn r_fs_context_new_shared(other: *const FsContext) -> *const FsContext {
    // SAFETY: `other` is a valid pointer from `Arc::into_raw()`.
    let other = BorrowedArc::from_raw(other);

    Arc::into_raw(other.clone())
}

#[no_mangle]
pub extern "C" fn r_fs_context_new_for_init() -> *const FsContext {
    Arc::into_raw(Arc::new(FsContext {
        fsroot: Dentry::kernel_root_dentry(),
        cwd: Spin::new(Dentry::kernel_root_dentry()),
        umask: Spin::new(0o022),
    }))
}
