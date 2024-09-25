use core::any::Any;

use alloc::{
    collections::btree_map::{BTreeMap, Entry},
    sync::{Arc, Weak},
};
use bindings::{
    statx, EEXIST, EINVAL, EIO, STATX_ATIME, STATX_BLOCKS, STATX_CTIME,
    STATX_GID, STATX_INO, STATX_MODE, STATX_MTIME, STATX_NLINK, STATX_SIZE,
    STATX_TYPE, STATX_UID, S_IFMT,
};

use super::{
    dentry::Dentry, s_isblk, s_ischr, vfs::Vfs, DevId, ReadDirCallback,
    TimeSpec,
};
use crate::prelude::*;

pub type Ino = u64;
pub type ISize = u64;
pub type Nlink = u64;
pub type Uid = u32;
pub type Gid = u32;
pub type Mode = u32;

#[repr(C)]
pub struct InodeData {
    pub ino: Ino,
    pub size: ISize,
    pub nlink: Nlink,

    pub atime: TimeSpec,
    pub mtime: TimeSpec,
    pub ctime: TimeSpec,

    pub uid: Uid,
    pub gid: Gid,
    pub mode: Mode,
}

impl InodeData {
    pub fn new(ino: Ino) -> Self {
        Self {
            ino,
            size: 0,
            nlink: 0,
            atime: TimeSpec::new(),
            mtime: TimeSpec::new(),
            ctime: TimeSpec::new(),
            uid: 0,
            gid: 0,
            mode: 0,
        }
    }
}

#[allow(unused_variables)]
pub trait Inode {
    fn idata(&self) -> &Mutex<InodeData>;
    fn vfs_weak(&self) -> Weak<Mutex<dyn Vfs>>;
    fn vfs_strong(&self) -> Option<Arc<Mutex<dyn Vfs>>>;
    fn as_any(&self) -> &dyn Any;

    fn readdir(
        &self,
        offset: usize,
        callback: &mut ReadDirCallback,
    ) -> KResult<usize>;

    fn statx(&self, stat: &mut statx, mask: u32) -> KResult<()> {
        let (fsdev, io_blksize) = {
            let vfs = self.vfs_strong().ok_or(EIO)?;
            let vfs = vfs.lock();
            (vfs.fs_devid(), vfs.io_blksize())
        };
        let devid = self.devid();

        let idata = self.idata().lock();

        if mask & STATX_NLINK != 0 {
            stat.stx_nlink = idata.nlink as _;
            stat.stx_mask |= STATX_NLINK;
        }

        if mask & STATX_ATIME != 0 {
            stat.stx_atime.tv_nsec = idata.atime.nsec as _;
            stat.stx_atime.tv_sec = idata.atime.sec as _;
            stat.stx_mask |= STATX_ATIME;
        }

        if mask & STATX_MTIME != 0 {
            stat.stx_mtime.tv_nsec = idata.mtime.nsec as _;
            stat.stx_mtime.tv_sec = idata.mtime.sec as _;
            stat.stx_mask |= STATX_MTIME;
        }

        if mask & STATX_CTIME != 0 {
            stat.stx_ctime.tv_nsec = idata.ctime.nsec as _;
            stat.stx_ctime.tv_sec = idata.ctime.sec as _;
            stat.stx_mask |= STATX_CTIME;
        }

        if mask & STATX_SIZE != 0 {
            stat.stx_size = idata.size as _;
            stat.stx_mask |= STATX_SIZE;
        }

        stat.stx_mode = 0;
        if mask & STATX_MODE != 0 {
            stat.stx_mode |= (idata.mode & !S_IFMT) as u16;
            stat.stx_mask |= STATX_MODE;
        }

        if mask & STATX_TYPE != 0 {
            stat.stx_mode |= (idata.mode & S_IFMT) as u16;
            if s_isblk(idata.mode) || s_ischr(idata.mode) {
                stat.stx_rdev_major = (devid? >> 8) & 0xff;
                stat.stx_rdev_minor = devid? & 0xff;
            }
            stat.stx_mask |= STATX_TYPE;
        }

        if mask & STATX_INO != 0 {
            stat.stx_ino = idata.ino as _;
            stat.stx_mask |= STATX_INO;
        }

        if mask & STATX_BLOCKS != 0 {
            stat.stx_blocks = (idata.size + 512 - 1) / 512;
            stat.stx_blksize = io_blksize as _;
            stat.stx_mask |= STATX_BLOCKS;
        }

        if mask & STATX_UID != 0 {
            stat.stx_uid = idata.uid as _;
            stat.stx_mask |= STATX_UID;
        }

        if mask & STATX_GID != 0 {
            stat.stx_gid = idata.gid as _;
            stat.stx_mask |= STATX_GID;
        }

        stat.stx_dev_major = (fsdev >> 8) & 0xff;
        stat.stx_dev_minor = fsdev & 0xff;

        // TODO: support more attributes
        stat.stx_attributes_mask = 0;

        Ok(())
    }

    fn creat(&self, at: &mut Dentry, mode: Mode) -> KResult<()> {
        Err(EINVAL)
    }

    fn mkdir(&self, at: &mut Dentry, mode: Mode) -> KResult<()> {
        Err(EINVAL)
    }

    fn mknod(&self, at: &mut Dentry, mode: Mode, dev: DevId) -> KResult<()> {
        Err(EINVAL)
    }

    fn unlink(&self, at: &mut Dentry) -> KResult<()> {
        Err(EINVAL)
    }

    fn symlink(&self, at: &mut Dentry, target: &str) -> KResult<()> {
        Err(EINVAL)
    }

    fn read(
        &self,
        buffer: &mut [u8],
        count: usize,
        offset: usize,
    ) -> KResult<usize> {
        Err(EINVAL)
    }

    fn write(&self, buffer: &[u8], offset: usize) -> KResult<usize> {
        Err(EINVAL)
    }

    fn devid(&self) -> KResult<DevId> {
        Err(EINVAL)
    }

    fn readlink(&self, buffer: &mut [u8]) -> KResult<usize> {
        Err(EINVAL)
    }

    fn truncate(&self, length: usize) -> KResult<()> {
        Err(EINVAL)
    }
}

pub struct InodeCache<Fs: Vfs> {
    cache: BTreeMap<Ino, Arc<dyn Inode>>,
    vfs: Weak<Mutex<Fs>>,
}

impl<Fs: Vfs> InodeCache<Fs> {
    pub fn new() -> Self {
        Self {
            cache: BTreeMap::new(),
            vfs: Weak::new(),
        }
    }

    pub fn get_vfs(&self) -> Weak<Mutex<Fs>> {
        self.vfs.clone()
    }

    pub fn set_vfs(&mut self, vfs: Weak<Mutex<Fs>>) {
        assert_eq!(self.vfs.upgrade().is_some(), false);

        self.vfs = vfs;
    }

    pub fn submit(
        &mut self,
        ino: Ino,
        inode: Arc<impl Inode + 'static>,
    ) -> KResult<Arc<dyn Inode>> {
        match self.cache.entry(ino) {
            Entry::Occupied(_) => Err(EEXIST), // TODO: log error to console
            Entry::Vacant(entry) => Ok(entry.insert(inode).clone()),
        }
    }

    pub fn get(&self, ino: Ino) -> Option<Arc<dyn Inode>> {
        self.cache.get(&ino).cloned()
    }

    pub fn free(&mut self, ino: Ino) {
        self.cache.remove(&ino);
    }
}
