use crate::prelude::*;

use alloc::sync::Arc;
use bindings::{dev_t, S_IFBLK, S_IFCHR, S_IFDIR, S_IFMT, S_IFREG};
use inode::{Inode, InodeData, Mode};

pub mod dentry;
pub mod ffi;
pub mod inode;
pub mod mount;
pub mod vfs;

pub type DevId = dev_t;

/// # Return
///
/// Return -1 if an error occurred
///
/// Return 0 if no more entry available
///
/// Otherwise, return bytes to be added to the offset
pub type ReadDirCallback =
    dyn FnMut(&str, &Arc<dyn Inode>, &InodeData, u8) -> KResult<i32>;

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

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct TimeSpec {
    pub sec: u64,
    pub nsec: u64,
}

impl TimeSpec {
    pub fn new() -> Self {
        Self { sec: 0, nsec: 0 }
    }
}
