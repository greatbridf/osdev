use crate::{
    io::copy_offset_count,
    kernel::vfs::{
        dentry::Dentry,
        inode::{Ino, Inode, InodeCache, InodeData, Mode},
        mount::{register_filesystem, Mount, MountCreator, MS_RDONLY},
        s_isblk, s_ischr,
        vfs::Vfs,
        DevId, ReadDirCallback, TimeSpec,
    },
    prelude::*,
};

use alloc::sync::{Arc, Weak};

use bindings::{
    fs::{D_DIRECTORY, D_LOADED, D_PRESENT, D_SYMLINK},
    EINVAL, EIO, EISDIR, ENODEV, ENOTDIR, EROFS, S_IFBLK, S_IFCHR, S_IFDIR,
    S_IFLNK, S_IFREG,
};

type TmpFsFile = Vec<u8>;
type TmpFsDirectory = Vec<(Ino, String)>;

enum TmpFsData {
    File(TmpFsFile),
    Device(DevId),
    Directory(TmpFsDirectory),
    Symlink(String),
}

struct TmpFsInode {
    idata: Mutex<InodeData>,
    fsdata: Mutex<TmpFsData>,
    vfs: Weak<Mutex<TmpFs>>,
}

impl TmpFsInode {
    pub fn new(
        idata: InodeData,
        fsdata: TmpFsData,
        vfs: Weak<Mutex<TmpFs>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            idata: Mutex::new(idata),
            fsdata: Mutex::new(fsdata),
            vfs,
        })
    }

    fn vfs(&self) -> KResult<Arc<Mutex<TmpFs>>> {
        self.vfs.upgrade().ok_or(EIO)
    }

    /// Link a child inode to the parent inode
    ///
    /// # Safety
    /// If parent is not a directory, this function will panic
    ///
    fn link_unchecked(
        parent_fsdata: &mut TmpFsData,
        parent_idata: &mut InodeData,
        name: &str,
        child_idata: &mut InodeData,
    ) {
        match parent_fsdata {
            TmpFsData::Directory(dir) => {
                dir.push((child_idata.ino, String::from(name)));

                parent_idata.size += size_of::<TmpFsData>() as u64;
                child_idata.nlink += 1;
            }

            _ => panic!("Parent is not a directory"),
        }
    }

    /// Link a inode to itself
    ///
    /// # Safety
    /// If the inode is not a directory, this function will panic
    ///
    fn self_link_unchecked(
        fsdata: &mut TmpFsData,
        idata: &mut InodeData,
        name: &str,
    ) {
        match fsdata {
            TmpFsData::Directory(dir) => {
                dir.push((idata.ino, String::from(name)));

                idata.size += size_of::<TmpFsData>() as u64;
                idata.nlink += 1;
            }

            _ => panic!("parent is not a directory"),
        }
    }
}

impl Inode for TmpFsInode {
    fn idata(&self) -> &Mutex<InodeData> {
        &self.idata
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn readdir(
        &self,
        offset: usize,
        callback: &mut ReadDirCallback,
    ) -> KResult<usize> {
        let _vfs = self.vfs.upgrade().ok_or(EIO)?;
        let vfs = _vfs.lock();

        match *self.fsdata.lock() {
            TmpFsData::Directory(ref dir) => {
                let icache = vfs.icache.lock();

                let mut nread = 0;

                for (ino, filename) in dir.iter().skip(offset) {
                    let inode = icache.get(*ino).unwrap();

                    let ret =
                        callback(filename, &inode, &inode.idata().lock(), 0)?;
                    if ret != 0 {
                        break;
                    }

                    nread += 1;
                }

                Ok(nread)
            }

            _ => Err(ENOTDIR),
        }
    }

    fn read(&self, buffer: &mut [u8], offset: usize) -> KResult<usize> {
        self.vfs()?;

        match *self.fsdata.lock() {
            TmpFsData::File(ref file) => Ok(copy_offset_count(
                file,
                buffer,
                offset as usize,
                buffer.len(),
            )),

            _ => Err(EINVAL),
        }
    }

    fn write(&self, buffer: &[u8], offset: usize) -> KResult<usize> {
        if self.vfs()?.lock().readonly {
            return Err(EROFS);
        }

        match *self.fsdata.lock() {
            TmpFsData::File(ref mut file) => {
                if file.len() < offset + buffer.len() {
                    file.resize(offset + buffer.len(), 0);
                }

                file[offset..offset + buffer.len()].copy_from_slice(&buffer);

                self.idata.lock().size = file.len() as u64;

                Ok(buffer.len())
            }

            _ => Err(EINVAL),
        }
    }

    fn creat(&self, at: &mut Dentry, mode: Mode) -> KResult<()> {
        let _vfs = self.vfs()?;
        let mut vfs = _vfs.lock();
        if vfs.readonly {
            return Err(EROFS);
        }

        {
            let self_fsdata = self.fsdata.lock();
            match *self_fsdata {
                TmpFsData::Directory(_) => {}
                _ => return Err(ENOTDIR),
            }
        }

        let ino = vfs.assign_ino();

        let file = {
            let mut locked_icache = vfs.icache.lock();
            let file = TmpFsInode::new(
                InodeData {
                    ino,
                    nlink: 0,
                    size: 0,
                    mode: S_IFREG | (mode & 0o777),
                    atime: TimeSpec::new(),
                    mtime: TimeSpec::new(),
                    ctime: TimeSpec::new(),
                    uid: 0,
                    gid: 0,
                },
                TmpFsData::File(vec![]),
                locked_icache.get_vfs(),
            );

            locked_icache.submit(ino, file.clone())?;

            file
        };

        {
            let mut self_fsdata = self.fsdata.lock();
            let mut self_idata = self.idata.lock();
            let mut child_idata = file.idata.lock();

            TmpFsInode::link_unchecked(
                &mut self_fsdata,
                &mut self_idata,
                at.get_name(),
                &mut child_idata,
            );
        }

        at.save_inode(file);
        at.flags |= D_PRESENT;

        Ok(())
    }

    fn mknod(&self, at: &mut Dentry, mode: Mode, dev: DevId) -> KResult<()> {
        let _vfs = self.vfs()?;
        let mut vfs = _vfs.lock();
        if vfs.readonly {
            return Err(EROFS);
        }

        if !s_ischr(mode) && !s_isblk(mode) {
            return Err(EINVAL);
        }

        {
            let self_fsdata = self.fsdata.lock();

            match *self_fsdata {
                TmpFsData::Directory(_) => {}
                _ => return Err(ENOTDIR),
            }
        }

        let ino = vfs.assign_ino();

        let file = {
            let mut locked_icache = vfs.icache.lock();
            let file = TmpFsInode::new(
                InodeData {
                    ino,
                    nlink: 0,
                    size: 0,
                    mode: mode & (0o777 | S_IFBLK | S_IFCHR),
                    atime: TimeSpec::new(),
                    mtime: TimeSpec::new(),
                    ctime: TimeSpec::new(),
                    uid: 0,
                    gid: 0,
                },
                TmpFsData::Device(dev),
                locked_icache.get_vfs(),
            );

            locked_icache.submit(ino, file.clone())?;

            file
        };

        {
            let mut self_fsdata = self.fsdata.lock();
            let mut self_idata = self.idata.lock();
            let mut child_idata = file.idata.lock();

            TmpFsInode::link_unchecked(
                &mut self_fsdata,
                &mut self_idata,
                at.get_name(),
                &mut child_idata,
            );
        }

        at.save_inode(file);
        at.flags |= D_PRESENT;

        Ok(())
    }

    fn mkdir(&self, at: &mut Dentry, mode: Mode) -> KResult<()> {
        let _vfs = self.vfs()?;
        let mut vfs = _vfs.lock();
        if vfs.readonly {
            return Err(EROFS);
        }

        {
            let self_fsdata = self.fsdata.lock();

            match *self_fsdata {
                TmpFsData::Directory(_) => {}
                _ => return Err(ENOTDIR),
            }
        }

        let ino = vfs.assign_ino();

        let dir = {
            let mut locked_icache = vfs.icache.lock();
            let file = TmpFsInode::new(
                InodeData {
                    ino,
                    nlink: 0,
                    size: 0,
                    mode: S_IFDIR | (mode & 0o777),
                    atime: TimeSpec::new(),
                    mtime: TimeSpec::new(),
                    ctime: TimeSpec::new(),
                    uid: 0,
                    gid: 0,
                },
                TmpFsData::Directory(vec![]),
                locked_icache.get_vfs(),
            );

            locked_icache.submit(ino, file.clone())?;

            file
        };

        {
            let mut self_fsdata = self.fsdata.lock();
            let mut self_idata = self.idata.lock();
            let mut child_fsdata = dir.fsdata.lock();
            let mut child_idata = dir.idata.lock();

            TmpFsInode::link_unchecked(
                &mut child_fsdata,
                &mut child_idata,
                "..",
                &mut self_idata,
            );

            TmpFsInode::self_link_unchecked(
                &mut child_fsdata,
                &mut child_idata,
                ".",
            );

            TmpFsInode::link_unchecked(
                &mut self_fsdata,
                &mut self_idata,
                at.get_name(),
                &mut child_idata,
            );
        }

        at.save_inode(dir);
        // TODO: try remove D_LOADED and check if it works
        at.flags |= D_PRESENT | D_DIRECTORY | D_LOADED;

        Ok(())
    }

    fn symlink(&self, at: &mut Dentry, target: &str) -> KResult<()> {
        let _vfs = self.vfs()?;
        let mut vfs = _vfs.lock();
        if vfs.readonly {
            return Err(EROFS);
        }

        {
            let self_fsdata = self.fsdata.lock();

            match *self_fsdata {
                TmpFsData::Directory(_) => {}
                _ => return Err(ENOTDIR),
            }
        }

        let ino = vfs.assign_ino();

        let file = {
            let mut locked_icache = vfs.icache.lock();
            let file = TmpFsInode::new(
                InodeData {
                    ino,
                    nlink: 0,
                    size: target.len() as u64,
                    mode: S_IFLNK | 0o777,
                    atime: TimeSpec::new(),
                    mtime: TimeSpec::new(),
                    ctime: TimeSpec::new(),
                    uid: 0,
                    gid: 0,
                },
                TmpFsData::Symlink(String::from(target)),
                locked_icache.get_vfs(),
            );

            locked_icache.submit(ino, file.clone())?;

            file
        };

        {
            let mut self_fsdata = self.fsdata.lock();
            let mut self_idata = self.idata.lock();
            let mut child_idata = file.idata.lock();

            TmpFsInode::link_unchecked(
                &mut self_fsdata,
                &mut self_idata,
                at.get_name(),
                &mut child_idata,
            );
        }

        at.save_inode(file);
        at.flags |= D_PRESENT | D_SYMLINK;

        Ok(())
    }

    fn readlink(&self, buffer: &mut [u8]) -> KResult<usize> {
        match *self.fsdata.lock() {
            TmpFsData::Symlink(ref target) => {
                let len = target.len().min(buffer.len());

                buffer[..len].copy_from_slice(target.as_bytes());

                Ok(len)
            }

            _ => Err(EINVAL),
        }
    }

    fn devid(&self) -> KResult<DevId> {
        match *self.fsdata.lock() {
            TmpFsData::Device(dev) => Ok(dev),
            _ => Err(ENODEV),
        }
    }

    fn truncate(&self, length: usize) -> KResult<()> {
        if self.vfs()?.lock().readonly {
            return Err(EROFS);
        }

        match *self.fsdata.lock() {
            TmpFsData::File(ref mut file) => {
                file.resize(length, 0);
                self.idata.lock().size = length as u64;

                Ok(())
            }

            _ => Err(EINVAL),
        }
    }

    fn unlink(&self, at: &mut Dentry) -> KResult<()> {
        if self.vfs()?.lock().readonly {
            return Err(EROFS);
        }

        let file = at.get_inode_clone();
        let file = file.as_any().downcast_ref::<TmpFsInode>().unwrap();

        match *file.fsdata.lock() {
            TmpFsData::Directory(_) => return Err(EISDIR),
            _ => {}
        }
        let file_data = file.idata.lock();

        let mut self_fsdata = self.fsdata.lock();

        match *self_fsdata {
            TmpFsData::Directory(ref mut dirs) => {
                let idx = 'label: {
                    for (idx, (ino, _)) in dirs.iter().enumerate() {
                        if *ino != file_data.ino {
                            continue;
                        }
                        break 'label idx;
                    }
                    panic!("file not found in directory");
                };

                drop(file_data);
                {
                    self.idata.lock().size -= size_of::<TmpFsData>() as u64;
                    file.idata.lock().nlink -= 1;
                }
                dirs.remove(idx);

                // TODO!!!: CHANGE THIS SINCE IT WILL CAUSE MEMORY LEAK
                // AND WILL CREATE A RACE CONDITION
                at.flags &= !D_PRESENT;
                at.take_inode();
                at.take_fs();

                Ok(())
            }
            _ => return Err(ENOTDIR),
        }
    }

    fn vfs_weak(&self) -> Weak<Mutex<dyn Vfs>> {
        self.vfs.clone()
    }

    fn vfs_strong(&self) -> Option<Arc<Mutex<dyn Vfs>>> {
        match self.vfs.upgrade() {
            Some(vfs) => Some(vfs),
            None => None,
        }
    }
}

/// # Lock order
/// vfs -> icache -> fsdata -> data
struct TmpFs {
    icache: Mutex<InodeCache<TmpFs>>,
    next_ino: Ino,
    readonly: bool,
}

impl TmpFs {
    fn assign_ino(&mut self) -> Ino {
        let ino = self.next_ino;
        self.next_ino += 1;

        ino
    }

    pub fn create(
        readonly: bool,
    ) -> KResult<(Arc<Mutex<TmpFs>>, Arc<TmpFsInode>)> {
        let tmpfs = Arc::new(Mutex::new(Self {
            icache: Mutex::new(InodeCache::new()),
            next_ino: 1,
            readonly,
        }));

        let root_inode = {
            let locked_tmpfs = tmpfs.lock();
            let mut locked_icache = locked_tmpfs.icache.lock();
            locked_icache.set_vfs(Arc::downgrade(&tmpfs));

            let file = TmpFsInode::new(
                InodeData {
                    ino: 0,
                    nlink: 0,
                    size: 0,
                    mode: S_IFDIR | 0o755,
                    atime: TimeSpec::new(),
                    mtime: TimeSpec::new(),
                    ctime: TimeSpec::new(),
                    uid: 0,
                    gid: 0,
                },
                TmpFsData::Directory(vec![]),
                locked_icache.get_vfs(),
            );

            locked_icache.submit(0, file.clone())?;

            file
        };

        {
            let mut fsdata = root_inode.fsdata.lock();
            let mut idata = root_inode.idata.lock();

            TmpFsInode::self_link_unchecked(&mut fsdata, &mut idata, ".");
            TmpFsInode::self_link_unchecked(&mut fsdata, &mut idata, "..");
        }

        Ok((tmpfs, root_inode))
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
    ) -> KResult<Mount> {
        let (fs, root_inode) = TmpFs::create(flags & MS_RDONLY != 0)?;

        Ok(Mount::new(fs, root_inode))
    }
}

pub fn init() {
    register_filesystem("tmpfs", Box::new(TmpFsMountCreator)).unwrap();
}
