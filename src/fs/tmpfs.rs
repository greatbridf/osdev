use core::sync::atomic::Ordering;

use crate::{
    io::Buffer,
    kernel::vfs::{
        dentry::Dentry,
        inode::{AtomicIno, Ino, Inode, InodeCache, InodeOps, Mode},
        mount::{register_filesystem, Mount, MountCreator, MS_RDONLY},
        s_isblk, s_ischr,
        vfs::Vfs,
        DevId, ReadDirCallback,
    },
    prelude::*,
};

use alloc::sync::Arc;

use bindings::{
    EINVAL, EIO, EISDIR, EROFS, S_IFBLK, S_IFCHR, S_IFDIR, S_IFLNK, S_IFREG,
};

struct FileOps {
    data: Mutex<Vec<u8>>,
}

struct NodeOps {
    devid: DevId,
}

impl NodeOps {
    fn new(devid: DevId) -> Self {
        Self { devid }
    }
}

impl InodeOps for NodeOps {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn devid(&self, _: &Inode) -> KResult<DevId> {
        Ok(self.devid)
    }
}

struct DirectoryOps {
    entries: Mutex<Vec<(Arc<[u8]>, Ino)>>,
}

impl DirectoryOps {
    fn new() -> Self {
        Self {
            entries: Mutex::new(vec![]),
        }
    }

    /// Locks the `inode.idata`
    fn link(&self, dir: &Inode, file: &Inode, name: Arc<[u8]>) -> KResult<()> {
        dir.idata.lock().size += 1;
        self.entries.lock().push((name, file.ino));

        file.idata.lock().nlink += 1;

        Ok(())
    }
}

impl InodeOps for DirectoryOps {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn readdir<'cb, 'r: 'cb>(
        &self,
        _: &Inode,
        offset: usize,
        callback: &ReadDirCallback<'cb>,
    ) -> KResult<usize> {
        Ok(self
            .entries
            .lock()
            .iter()
            .skip(offset)
            .take_while(|(name, ino)| callback(name, *ino).is_ok())
            .count())
    }

    fn creat(&self, dir: &Inode, at: &Arc<Dentry>, mode: Mode) -> KResult<()> {
        let vfs = dir.vfs.upgrade().ok_or(EIO)?;
        let vfs = vfs.as_any().downcast_ref::<TmpFs>().unwrap();

        if vfs.readonly {
            return Err(EROFS);
        }

        let ino = vfs.assign_ino();
        let file = vfs.icache.lock().alloc_file(ino, mode)?;

        self.link(dir, file.as_ref(), at.name().clone())?;
        at.save_reg(file)
    }

    fn mknod(
        &self,
        dir: &Inode,
        at: &Arc<Dentry>,
        mode: Mode,
        dev: DevId,
    ) -> KResult<()> {
        let vfs = dir.vfs.upgrade().ok_or(EIO)?;
        let vfs = vfs.as_any().downcast_ref::<TmpFs>().unwrap();

        if vfs.readonly {
            return Err(EROFS);
        }

        if !s_ischr(mode) && !s_isblk(mode) {
            return Err(EINVAL);
        }

        let ino = vfs.assign_ino();
        let mut icache = vfs.icache.lock();
        let file = icache.alloc(ino, Box::new(NodeOps::new(dev)));
        file.idata.lock().mode = mode & (0o777 | S_IFBLK | S_IFCHR);
        icache.submit(&file)?;

        self.link(dir, file.as_ref(), at.name().clone())?;
        at.save_reg(file)
    }

    fn symlink(
        &self,
        dir: &Inode,
        at: &Arc<Dentry>,
        target: &[u8],
    ) -> KResult<()> {
        let vfs = dir.vfs.upgrade().ok_or(EIO)?;
        let vfs = vfs.as_any().downcast_ref::<TmpFs>().unwrap();

        if vfs.readonly {
            return Err(EROFS);
        }

        let ino = vfs.assign_ino();
        let mut icache = vfs.icache.lock();

        let target_len = target.len() as u64;

        let file =
            icache.alloc(ino, Box::new(SymlinkOps::new(Arc::from(target))));
        {
            let mut idata = file.idata.lock();
            idata.mode = S_IFLNK | 0o777;
            idata.size = target_len;
        }
        icache.submit(&file)?;

        self.link(dir, file.as_ref(), at.name().clone())?;
        at.save_symlink(file)
    }

    fn mkdir(&self, dir: &Inode, at: &Arc<Dentry>, mode: Mode) -> KResult<()> {
        let vfs = dir.vfs.upgrade().ok_or(EIO)?;
        let vfs = vfs.as_any().downcast_ref::<TmpFs>().unwrap();

        if vfs.readonly {
            return Err(EROFS);
        }

        let ino = vfs.assign_ino();
        let mut icache = vfs.icache.lock();

        let mut newdir_ops = DirectoryOps::new();
        let entries = newdir_ops.entries.get_mut();
        entries.push((Arc::from(b".".as_slice()), ino));
        entries.push((Arc::from(b"..".as_slice()), dir.ino));

        let newdir = icache.alloc(ino, Box::new(newdir_ops));
        {
            let mut newdir_idata = newdir.idata.lock();
            newdir_idata.mode = S_IFDIR | (mode & 0o777);
            newdir_idata.nlink = 1;
            newdir_idata.size = 2;
        }

        icache.submit(&newdir)?;
        dir.idata.lock().nlink += 1; // link from `newdir` to `dir`, (or parent)

        self.link(dir, newdir.as_ref(), at.name().clone())?;
        at.save_dir(newdir)
    }

    fn unlink(&self, dir: &Inode, at: &Arc<Dentry>) -> KResult<()> {
        let vfs = dir.vfs.upgrade().ok_or(EIO)?;
        let vfs = vfs.as_any().downcast_ref::<TmpFs>().unwrap();

        if vfs.readonly {
            return Err(EROFS);
        }

        let file = at.get_inode()?;

        let mut file_idata = file.idata.lock();

        if file_idata.mode & S_IFDIR != 0 {
            return Err(EISDIR);
        }

        let mut self_idata = dir.idata.lock();
        let mut entries = self.entries.lock();

        let idx = entries
            .iter()
            .position(|(_, ino)| *ino == file.ino)
            .expect("file not found in directory");

        self_idata.size -= 1;
        file_idata.nlink -= 1;
        entries.remove(idx);

        at.invalidate()
    }
}

struct SymlinkOps {
    target: Arc<[u8]>,
}

impl SymlinkOps {
    fn new(target: Arc<[u8]>) -> Self {
        Self { target }
    }
}

impl InodeOps for SymlinkOps {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn readlink(&self, _: &Inode, buffer: &mut dyn Buffer) -> KResult<usize> {
        buffer
            .fill(self.target.as_ref())
            .map(|result| result.allow_partial())
    }
}

impl FileOps {
    fn new() -> Self {
        Self {
            data: Mutex::new(vec![]),
        }
    }
}

impl InodeOps for FileOps {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn read(
        &self,
        _: &Inode,
        buffer: &mut dyn Buffer,
        offset: usize,
    ) -> KResult<usize> {
        let data = self.data.lock();
        let data = data.split_at_checked(offset).ok_or(EINVAL)?.1;

        buffer.fill(data).map(|result| result.allow_partial())
    }

    fn write(
        &self,
        inode: &Inode,
        buffer: &[u8],
        offset: usize,
    ) -> KResult<usize> {
        let mut idata = inode.idata.lock();
        let mut data = self.data.lock();

        if data.len() < offset + buffer.len() {
            data.resize(offset + buffer.len(), 0);
        }

        data[offset..offset + buffer.len()].copy_from_slice(&buffer);
        idata.size = data.len() as u64;

        Ok(buffer.len())
    }

    fn truncate(&self, inode: &Inode, length: usize) -> KResult<()> {
        let mut idata = inode.idata.lock();

        idata.size = length as u64;
        self.data.lock().resize(length, 0);

        Ok(())
    }
}

/// # Lock order
/// `vfs` -> `icache` -> `idata` -> `*ops`.`*data`
struct TmpFs {
    icache: Mutex<InodeCache<TmpFs>>,
    next_ino: AtomicIno,
    readonly: bool,
}

impl InodeCache<TmpFs> {
    fn alloc_file(&mut self, ino: Ino, mode: Mode) -> KResult<Arc<Inode>> {
        let file = self.alloc(ino, Box::new(FileOps::new()));
        file.idata.lock().mode = S_IFREG | (mode & 0o777);

        self.submit(&file)?;

        Ok(file)
    }
}

impl TmpFs {
    fn assign_ino(&self) -> Ino {
        self.next_ino.fetch_add(1, Ordering::SeqCst)
    }

    pub fn create(readonly: bool) -> KResult<(Arc<TmpFs>, Arc<Inode>)> {
        let tmpfs = Arc::new_cyclic(|weak| Self {
            icache: Mutex::new(InodeCache::new(weak.clone())),
            next_ino: AtomicIno::new(1),
            readonly,
        });

        let mut dir = DirectoryOps::new();
        let entries = dir.entries.get_mut();
        entries.push((Arc::from(b".".as_slice()), 0));
        entries.push((Arc::from(b"..".as_slice()), 0));

        let root_dir = {
            let mut icache = tmpfs.icache.lock();
            let root_dir = icache.alloc(0, Box::new(dir));
            {
                let mut idata = root_dir.idata.lock();

                idata.mode = S_IFDIR | 0o755;
                idata.nlink = 2;
                idata.size = 2;
            }

            icache.submit(&root_dir)?;

            root_dir
        };

        Ok((tmpfs, root_dir))
    }
}

impl Vfs for TmpFs {
    fn io_blksize(&self) -> usize {
        4096
    }

    fn fs_devid(&self) -> DevId {
        2
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct TmpFsMountCreator;

impl MountCreator for TmpFsMountCreator {
    fn create_mount(
        &self,
        _source: &str,
        flags: u64,
        _data: &[u8],
        mp: &Arc<Dentry>,
    ) -> KResult<Mount> {
        let (fs, root_inode) = TmpFs::create(flags & MS_RDONLY != 0)?;

        Mount::new(mp, fs, root_inode)
    }
}

pub fn init() {
    register_filesystem("tmpfs", Box::new(TmpFsMountCreator)).unwrap();
}
