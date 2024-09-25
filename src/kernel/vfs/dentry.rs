use crate::prelude::*;

use core::ops::{Deref, DerefMut};

use alloc::{
    boxed::Box,
    sync::{Arc, Weak},
};

use crate::io::get_cxx_std_string;

use super::{
    bindings::{fs, EFAULT},
    inode::Inode,
    vfs::Vfs,
};

#[repr(C)]
pub struct DentryInner {
    fs: *const Mutex<dyn Vfs>,
    inode: *const dyn Inode,

    cache: *mut fs::dcache,
    pub parent: *mut DentryInner,

    prev: *mut DentryInner,
    next: *mut DentryInner,

    pub flags: u64,
    pub hash: u64,

    refcount: u64,
    pub name: [u64; 4],
}

pub struct Dentry {
    raw: *mut DentryInner,
}

/// We assume that the inode is owned and can not be invalidated
/// during our access to it.
///
pub fn raw_inode_clone(handle: *const *const dyn Inode) -> Arc<dyn Inode> {
    assert!(!handle.is_null());
    unsafe {
        Arc::increment_strong_count(*handle);
        let inode = Arc::from_raw(*handle);
        inode
    }
}

// TODO!!!: CHANGE THIS
unsafe impl Sync for Dentry {}
unsafe impl Send for Dentry {}

impl Dentry {
    pub fn from_raw(raw: *mut DentryInner) -> KResult<Self> {
        if raw.is_null() {
            return Err(EFAULT);
        }

        Ok(Dentry {
            raw: unsafe { fs::d_get1(raw as *mut _) as *mut _ },
        })
    }

    pub fn parent_from_raw(raw: *mut DentryInner) -> KResult<Self> {
        Dentry::from_raw(unsafe { raw.as_ref() }.ok_or(EFAULT)?.parent)
    }

    pub fn leak(self) -> *mut DentryInner {
        let raw = self.raw;
        core::mem::forget(self);
        raw
    }

    pub fn get_name(&self) -> &str {
        get_cxx_std_string(&self.name).unwrap()
    }

    pub fn save_inode(&mut self, inode: Arc<dyn Inode>) {
        let vfs = inode.vfs_weak();
        self.inode = Arc::into_raw(inode);
        self.fs = Weak::into_raw(vfs);
    }

    pub fn get_inode_clone(&self) -> Arc<dyn Inode> {
        raw_inode_clone(&self.inode)
    }

    pub fn take_inode(&self) -> Arc<dyn Inode> {
        unsafe { Arc::from_raw(self.inode) }
    }

    pub fn take_fs(&self) -> Arc<Mutex<dyn Vfs>> {
        unsafe { Arc::from_raw(self.fs) }
    }
}

impl Deref for Dentry {
    type Target = DentryInner;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.raw }
    }
}

impl DerefMut for Dentry {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.raw }
    }
}

impl Clone for Dentry {
    fn clone(&self) -> Self {
        unsafe {
            fs::d_get1(self.raw as *mut _) as *mut _;
        }
        Dentry { raw: self.raw }
    }
}

impl Drop for Dentry {
    fn drop(&mut self) {
        unsafe {
            fs::d_put(self.raw as *mut _);
        }
    }
}

impl PartialEq for Dentry {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl PartialOrd for Dentry {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.raw.partial_cmp(&other.raw)
    }
}

impl Eq for Dentry {}

impl Ord for Dentry {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.raw.cmp(&other.raw)
    }
}

const DCACHE_HASH_BITS: i32 = 8;

pub struct DentryCache {
    cache: Box<fs::dcache>,
}

unsafe impl Sync for DentryCache {}
unsafe impl Send for DentryCache {}

impl DentryCache {
    pub fn new() -> Self {
        let mut dcache = Self {
            cache: Box::new(fs::dcache {
                arr: core::ptr::null_mut(),
                hash_bits: Default::default(),
                size: Default::default(),
            }),
        };

        unsafe {
            fs::dcache_init(
                core::ptr::from_mut(&mut dcache.cache),
                DCACHE_HASH_BITS,
            )
        };

        dcache
    }

    pub fn alloc(&mut self) -> Dentry {
        let raw =
            unsafe { fs::dcache_alloc(core::ptr::from_mut(&mut self.cache)) };
        assert!(!raw.is_null());
        Dentry { raw: raw as *mut _ }
    }

    pub fn insert_root(&mut self, root: &mut Dentry) {
        unsafe {
            fs::dcache_init_root(
                core::ptr::from_mut(&mut self.cache),
                root.raw as *mut _,
            )
        }
    }
}

impl Drop for DentryCache {
    fn drop(&mut self) {
        unsafe {
            fs::dcache_drop(core::ptr::from_mut(&mut self.cache));
        }
    }
}
