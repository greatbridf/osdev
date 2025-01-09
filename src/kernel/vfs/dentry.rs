pub mod dcache;

use core::{
    hash::{BuildHasher, BuildHasherDefault, Hasher},
    ops::ControlFlow,
    sync::atomic::{AtomicPtr, Ordering},
};

use crate::{
    hash::KernelHasher,
    io::{Buffer, ByteBuffer},
    kernel::{block::BlockDevice, CharDevice},
    path::{Path, PathComponent},
    prelude::*,
    rcu::{RCUNode, RCUPointer},
};

use alloc::sync::Arc;
use bindings::{
    statx, EEXIST, EINVAL, EISDIR, ELOOP, ENOENT, ENOTDIR, EPERM, ERANGE, O_CREAT, O_EXCL,
};

use super::{
    inode::{Ino, Inode, Mode, WriteOffset},
    s_isblk, s_ischr, s_isdir, s_isreg, DevId, FsContext,
};

struct DentryData {
    inode: Arc<dyn Inode>,
    flags: u64,
}

/// # Safety
///
/// We wrap `Dentry` in `Arc` to ensure that the `Dentry` is not dropped while it is still in use.
///
/// Since a `Dentry` is created and marked as live(some data is saved to it), it keeps alive until
/// the last reference is dropped.
pub struct Dentry {
    // Const after insertion into dcache
    parent: Arc<Dentry>,
    name: Arc<[u8]>,
    hash: u64,

    // Used by the dentry cache
    prev: AtomicPtr<Dentry>,
    next: AtomicPtr<Dentry>,

    // RCU Mutable
    data: RCUPointer<DentryData>,
}

impl core::fmt::Debug for Dentry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Dentry")
            .field("name", &String::from_utf8_lossy(&self.name))
            .field("parent", &String::from_utf8_lossy(&self.parent.name))
            .finish()
    }
}

const D_DIRECTORY: u64 = 1;
#[allow(dead_code)]
const D_MOUNTPOINT: u64 = 2;
const D_SYMLINK: u64 = 4;
const D_REGULAR: u64 = 8;

impl RCUNode<Dentry> for Dentry {
    fn rcu_prev(&self) -> &AtomicPtr<Self> {
        &self.prev
    }

    fn rcu_next(&self) -> &AtomicPtr<Self> {
        &self.next
    }
}

impl Dentry {
    fn rehash(self: &Arc<Self>) -> u64 {
        let builder: BuildHasherDefault<KernelHasher> = Default::default();
        let mut hasher = builder.build_hasher();

        hasher.write_usize(self.parent_addr() as usize);
        hasher.write(self.name.as_ref());

        hasher.finish()
    }

    fn find(self: &Arc<Self>, name: &[u8]) -> KResult<Arc<Self>> {
        let data = self.data.load();
        let data = data.as_ref().ok_or(ENOENT)?;

        if data.flags & D_DIRECTORY == 0 {
            return Err(ENOTDIR);
        }

        match name {
            b"." => Ok(self.clone()),
            b".." => Ok(self.parent.clone()),
            _ => {
                let dentry = Dentry::create(self.clone(), name);
                Ok(dcache::d_find_fast(&dentry).unwrap_or_else(|| {
                    dcache::d_try_revalidate(&dentry);
                    dcache::d_add(&dentry);

                    dentry
                }))
            }
        }
    }
}

impl Dentry {
    pub fn create(parent: Arc<Dentry>, name: &[u8]) -> Arc<Self> {
        let mut val = Arc::new(Self {
            parent,
            name: Arc::from(name),
            hash: 0,
            prev: AtomicPtr::default(),
            next: AtomicPtr::default(),
            data: RCUPointer::empty(),
        });
        let hash = val.rehash();
        let val_mut = Arc::get_mut(&mut val).unwrap();
        val_mut.hash = hash;

        val
    }

    /// Check the equality of two denties inside the same dentry cache hash group
    /// where `other` is identified by `hash`, `parent` and `name`
    ///
    fn hash_eq(self: &Arc<Self>, other: &Arc<Self>) -> bool {
        self.hash == other.hash
            && self.parent_addr() == other.parent_addr()
            && self.name == other.name
    }

    pub fn name(&self) -> &Arc<[u8]> {
        &self.name
    }

    pub fn parent(&self) -> &Arc<Self> {
        &self.parent
    }

    pub fn parent_addr(&self) -> *const Self {
        Arc::as_ptr(&self.parent)
    }

    fn save_data(&self, inode: Arc<dyn Inode>, flags: u64) -> KResult<()> {
        let new = DentryData { inode, flags };

        // TODO!!!: We don't actually need to use `RCUPointer` here
        // Safety: this function may only be called from `create`-like functions which requires the
        // superblock's write locks to be held, so only one creation can happen at a time and we
        // can't get a reference to the old data.
        let old = unsafe { self.data.swap(Some(Arc::new(new))) };
        assert!(old.is_none());

        Ok(())
    }

    pub fn save_reg(&self, file: Arc<dyn Inode>) -> KResult<()> {
        self.save_data(file, D_REGULAR)
    }

    pub fn save_symlink(&self, link: Arc<dyn Inode>) -> KResult<()> {
        self.save_data(link, D_SYMLINK)
    }

    pub fn save_dir(&self, dir: Arc<dyn Inode>) -> KResult<()> {
        self.save_data(dir, D_DIRECTORY)
    }

    pub fn get_inode(&self) -> KResult<Arc<dyn Inode>> {
        self.data
            .load()
            .as_ref()
            .ok_or(ENOENT)
            .map(|data| data.inode.clone())
    }

    pub fn is_directory(&self) -> bool {
        let data = self.data.load();
        data.as_ref()
            .map_or(false, |data| data.flags & D_DIRECTORY != 0)
    }

    pub fn is_valid(&self) -> bool {
        self.data.load().is_some()
    }

    pub fn open_check(self: &Arc<Self>, flags: u32, mode: Mode) -> KResult<()> {
        let data = self.data.load();
        let create = flags & O_CREAT != 0;
        let excl = flags & O_EXCL != 0;

        if data.is_some() {
            if create && excl {
                return Err(EEXIST);
            }
            return Ok(());
        } else {
            if !create {
                return Err(ENOENT);
            }

            let parent = self.parent().get_inode()?;
            parent.creat(self, mode as u32)
        }
    }
}

impl Dentry {
    fn resolve_directory(
        context: &FsContext,
        dentry: Arc<Self>,
        nrecur: u32,
    ) -> KResult<Arc<Self>> {
        if nrecur >= 16 {
            return Err(ELOOP);
        }

        let data = dentry.data.load();
        let data = data.as_ref().ok_or(ENOENT)?;

        match data.flags {
            flags if flags & D_REGULAR != 0 => Err(ENOTDIR),
            flags if flags & D_DIRECTORY != 0 => Ok(dentry),
            flags if flags & D_SYMLINK != 0 => {
                let mut buffer = [0u8; 256];
                let mut buffer = ByteBuffer::new(&mut buffer);

                data.inode.readlink(&mut buffer)?;
                let path = Path::new(buffer.data())?;

                let dentry = Self::open_recursive(context, &dentry.parent, path, true, nrecur + 1)?;

                Self::resolve_directory(context, dentry, nrecur + 1)
            }
            _ => panic!("Invalid dentry flags"),
        }
    }

    pub fn open_recursive(
        context: &FsContext,
        cwd: &Arc<Self>,
        path: Path,
        follow: bool,
        nrecur: u32,
    ) -> KResult<Arc<Self>> {
        // too many recursive search layers will cause stack overflow
        // so we use 16 for now
        if nrecur >= 16 {
            return Err(ELOOP);
        }

        let mut cwd = if path.is_absolute() {
            context.fsroot.clone()
        } else {
            cwd.clone()
        };

        for item in path.iter() {
            if let PathComponent::TrailingEmpty = item {
                if cwd.data.load().as_ref().is_none() {
                    return Ok(cwd);
                }
            }

            cwd = Self::resolve_directory(context, cwd, nrecur)?;

            match item {
                PathComponent::TrailingEmpty | PathComponent::Current => {} // pass
                PathComponent::Parent => {
                    if !cwd.hash_eq(&context.fsroot) {
                        cwd = Self::resolve_directory(context, cwd.parent.clone(), nrecur)?;
                    }
                    continue;
                }
                PathComponent::Name(name) => {
                    cwd = cwd.find(name)?;
                }
            }
        }

        if follow {
            let data = cwd.data.load();

            if let Some(data) = data.as_ref() {
                if data.flags & D_SYMLINK != 0 {
                    let data = cwd.data.load();
                    let data = data.as_ref().unwrap();
                    let mut buffer = [0u8; 256];
                    let mut buffer = ByteBuffer::new(&mut buffer);

                    data.inode.readlink(&mut buffer)?;
                    let path = Path::new(buffer.data())?;

                    cwd = Self::open_recursive(context, &cwd.parent, path, true, nrecur + 1)?;
                }
            }
        }

        Ok(cwd)
    }

    pub fn open(context: &FsContext, path: Path, follow_symlinks: bool) -> KResult<Arc<Self>> {
        let cwd = context.cwd.lock().clone();
        Dentry::open_recursive(context, &cwd, path, follow_symlinks, 0)
    }

    pub fn open_at(
        context: &FsContext,
        at: &Arc<Self>,
        path: Path,
        follow_symlinks: bool,
    ) -> KResult<Arc<Self>> {
        Dentry::open_recursive(context, at, path, follow_symlinks, 0)
    }

    pub fn get_path(
        self: &Arc<Dentry>,
        context: &FsContext,
        buffer: &mut dyn Buffer,
    ) -> KResult<()> {
        let mut dentry = self;
        let root = &context.fsroot;

        let mut path = vec![];

        while Arc::as_ptr(dentry) != Arc::as_ptr(root) {
            if path.len() > 32 {
                return Err(ELOOP);
            }

            path.push(dentry.name().clone());
            dentry = dentry.parent();
        }

        buffer.fill(b"/")?.ok_or(ERANGE)?;
        for item in path.iter().rev().map(|name| name.as_ref()) {
            buffer.fill(item)?.ok_or(ERANGE)?;
            buffer.fill(b"/")?.ok_or(ERANGE)?;
        }

        buffer.fill(&[0])?.ok_or(ERANGE)?;

        Ok(())
    }
}

impl Dentry {
    pub fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        let inode = self.get_inode()?;

        // Safety: Changing mode alone will have no effect on the file's contents
        match inode.mode.load(Ordering::Relaxed) {
            mode if s_isdir(mode) => Err(EISDIR),
            mode if s_isreg(mode) => inode.read(buffer, offset),
            mode if s_isblk(mode) => {
                let device = BlockDevice::get(inode.devid()?)?;
                Ok(device.read_some(offset, buffer)?.allow_partial())
            }
            mode if s_ischr(mode) => {
                let device = CharDevice::get(inode.devid()?).ok_or(EPERM)?;
                device.read(buffer)
            }
            _ => Err(EINVAL),
        }
    }

    pub fn write(&self, buffer: &[u8], offset: WriteOffset) -> KResult<usize> {
        let inode = self.get_inode()?;
        // Safety: Changing mode alone will have no effect on the file's contents
        match inode.mode.load(Ordering::Relaxed) {
            mode if s_isdir(mode) => Err(EISDIR),
            mode if s_isreg(mode) => inode.write(buffer, offset),
            mode if s_isblk(mode) => Err(EINVAL), // TODO
            mode if s_ischr(mode) => CharDevice::get(inode.devid()?).ok_or(EPERM)?.write(buffer),
            _ => Err(EINVAL),
        }
    }

    pub fn readdir<F>(&self, offset: usize, mut callback: F) -> KResult<usize>
    where
        F: FnMut(&[u8], Ino) -> KResult<ControlFlow<(), ()>>,
    {
        self.get_inode()?.do_readdir(offset, &mut callback)
    }

    pub fn mkdir(&self, mode: Mode) -> KResult<()> {
        if self.get_inode().is_ok() {
            Err(EEXIST)
        } else {
            self.parent.get_inode().unwrap().mkdir(self, mode)
        }
    }

    pub fn statx(&self, stat: &mut statx, mask: u32) -> KResult<()> {
        self.get_inode()?.statx(stat, mask)
    }

    pub fn truncate(&self, size: usize) -> KResult<()> {
        self.get_inode()?.truncate(size)
    }

    pub fn unlink(self: &Arc<Self>) -> KResult<()> {
        if self.get_inode().is_err() {
            Err(ENOENT)
        } else {
            self.parent.get_inode().unwrap().unlink(self)
        }
    }

    pub fn symlink(self: &Arc<Self>, link: &[u8]) -> KResult<()> {
        if self.get_inode().is_ok() {
            Err(EEXIST)
        } else {
            self.parent.get_inode().unwrap().symlink(self, link)
        }
    }

    pub fn readlink(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        self.get_inode()?.readlink(buffer)
    }

    pub fn mknod(&self, mode: Mode, devid: DevId) -> KResult<()> {
        if self.get_inode().is_ok() {
            Err(EEXIST)
        } else {
            self.parent.get_inode().unwrap().mknod(self, mode, devid)
        }
    }
}
