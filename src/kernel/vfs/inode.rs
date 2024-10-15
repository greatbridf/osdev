use core::{ops::Deref, sync::atomic::AtomicU64};

use alloc::{
    collections::btree_map::{BTreeMap, Entry},
    sync::{Arc, Weak},
};
use bindings::{
    statx, EEXIST, EINVAL, EIO, EISDIR, ENOTDIR, EPERM, STATX_ATIME,
    STATX_BLOCKS, STATX_CTIME, STATX_GID, STATX_INO, STATX_MODE, STATX_MTIME,
    STATX_NLINK, STATX_SIZE, STATX_TYPE, STATX_UID, S_IFDIR, S_IFMT,
};

use super::{
    dentry::Dentry, s_isblk, s_ischr, vfs::Vfs, DevId, ReadDirCallback,
    TimeSpec,
};
use crate::{io::Buffer, prelude::*};

pub type Ino = u64;
pub type AtomicIno = AtomicU64;
pub type ISize = u64;
pub type Nlink = u64;
pub type Uid = u32;
pub type Gid = u32;
pub type Mode = u32;

#[repr(C)]
#[derive(Default)]
pub struct InodeData {
    pub size: ISize,
    pub nlink: Nlink,

    pub uid: Uid,
    pub gid: Gid,
    pub mode: Mode,

    pub atime: TimeSpec,
    pub mtime: TimeSpec,
    pub ctime: TimeSpec,
}

pub struct Inode {
    pub ino: Ino,
    pub vfs: Weak<dyn Vfs>,

    pub idata: Mutex<InodeData>,
    pub ops: Box<dyn InodeOps>,
}

impl Deref for Inode {
    type Target = dyn InodeOps;

    fn deref(&self) -> &Self::Target {
        self.ops.as_ref()
    }
}

#[allow(unused_variables)]
pub trait InodeOps: Send + Sync {
    fn as_any(&self) -> &dyn Any;

    fn lookup(
        &self,
        dir: &Inode,
        dentry: &Arc<Dentry>,
    ) -> KResult<Option<Arc<Inode>>> {
        if dir.idata.lock().mode & S_IFDIR == 0 {
            Err(ENOTDIR)
        } else {
            Err(EPERM)
        }
    }

    fn creat(&self, dir: &Inode, at: &Arc<Dentry>, mode: Mode) -> KResult<()> {
        if dir.idata.lock().mode & S_IFDIR == 0 {
            Err(ENOTDIR)
        } else {
            Err(EPERM)
        }
    }

    fn mkdir(&self, dir: &Inode, at: &Arc<Dentry>, mode: Mode) -> KResult<()> {
        if dir.idata.lock().mode & S_IFDIR == 0 {
            Err(ENOTDIR)
        } else {
            Err(EPERM)
        }
    }

    fn mknod(
        &self,
        dir: &Inode,
        at: &Arc<Dentry>,
        mode: Mode,
        dev: DevId,
    ) -> KResult<()> {
        if dir.idata.lock().mode & S_IFDIR == 0 {
            Err(ENOTDIR)
        } else {
            Err(EPERM)
        }
    }

    fn unlink(&self, dir: &Inode, at: &Arc<Dentry>) -> KResult<()> {
        if dir.idata.lock().mode & S_IFDIR == 0 {
            Err(ENOTDIR)
        } else {
            Err(EPERM)
        }
    }

    fn symlink(
        &self,
        dir: &Inode,
        at: &Arc<Dentry>,
        target: &[u8],
    ) -> KResult<()> {
        if dir.idata.lock().mode & S_IFDIR == 0 {
            Err(ENOTDIR)
        } else {
            Err(EPERM)
        }
    }

    fn read(
        &self,
        inode: &Inode,
        buffer: &mut dyn Buffer,
        offset: usize,
    ) -> KResult<usize> {
        if inode.idata.lock().mode & S_IFDIR != 0 {
            Err(EISDIR)
        } else {
            Err(EINVAL)
        }
    }

    fn write(
        &self,
        inode: &Inode,
        buffer: &[u8],
        offset: usize,
    ) -> KResult<usize> {
        if inode.idata.lock().mode & S_IFDIR != 0 {
            Err(EISDIR)
        } else {
            Err(EINVAL)
        }
    }

    fn devid(&self, inode: &Inode) -> KResult<DevId> {
        if inode.idata.lock().mode & S_IFDIR != 0 {
            Err(EISDIR)
        } else {
            Err(EINVAL)
        }
    }

    fn readlink(
        &self,
        inode: &Inode,
        buffer: &mut dyn Buffer,
    ) -> KResult<usize> {
        Err(EINVAL)
    }

    fn truncate(&self, inode: &Inode, length: usize) -> KResult<()> {
        if inode.idata.lock().mode & S_IFDIR != 0 {
            Err(EISDIR)
        } else {
            Err(EPERM)
        }
    }

    fn readdir<'cb, 'r: 'cb>(
        &'r self,
        inode: &'r Inode,
        offset: usize,
        callback: &ReadDirCallback<'cb>,
    ) -> KResult<usize> {
        if inode.idata.lock().mode & S_IFDIR == 0 {
            Err(ENOTDIR)
        } else {
            Err(EPERM)
        }
    }

    fn statx(&self, inode: &Inode, stat: &mut statx, mask: u32) -> KResult<()> {
        let (fsdev, io_blksize) = {
            let vfs = inode.vfs.upgrade().ok_or(EIO)?;
            (vfs.fs_devid(), vfs.io_blksize())
        };
        let devid = self.devid(inode);

        let idata = inode.idata.lock();

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
            stat.stx_ino = inode.ino as _;
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
}

pub struct InodeCache<Fs: Vfs + 'static> {
    cache: BTreeMap<Ino, Arc<Inode>>,
    vfs: Weak<Fs>,
}

impl<Fs: Vfs> InodeCache<Fs> {
    pub fn new(vfs: Weak<Fs>) -> Self {
        Self {
            cache: BTreeMap::new(),
            vfs,
        }
    }

    pub fn vfs(&self) -> Weak<Fs> {
        self.vfs.clone()
    }

    pub fn alloc(&self, ino: Ino, ops: Box<dyn InodeOps>) -> Arc<Inode> {
        Arc::new(Inode {
            ino,
            vfs: self.vfs.clone(),
            idata: Mutex::new(InodeData::default()),
            ops,
        })
    }

    pub fn submit(&mut self, inode: &Arc<Inode>) -> KResult<()> {
        match self.cache.entry(inode.ino) {
            Entry::Occupied(_) => Err(EEXIST),
            Entry::Vacant(entry) => {
                entry.insert(inode.clone());
                Ok(())
            }
        }
    }

    pub fn get(&self, ino: Ino) -> Option<Arc<Inode>> {
        self.cache.get(&ino).cloned()
    }

    pub fn free(&mut self, ino: Ino) {
        self.cache.remove(&ino);
    }
}
