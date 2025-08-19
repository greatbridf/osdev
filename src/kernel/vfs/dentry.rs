pub mod dcache;

use super::{
    inode::{Ino, Inode, Mode, RenameData, WriteOffset},
    DevId, FsContext,
};
use crate::{
    hash::KernelHasher,
    io::{Buffer, ByteBuffer},
    kernel::{block::BlockDevice, CharDevice},
    path::{Path, PathComponent},
    prelude::*,
    rcu::{RCUNode, RCUPointer, RCUReadGuard},
};
use crate::{
    io::Stream,
    kernel::constants::{EEXIST, EINVAL, EIO, EISDIR, ELOOP, ENOENT, ENOTDIR, EPERM, ERANGE},
};
use alloc::sync::{Arc, Weak};
use core::{
    fmt,
    hash::{BuildHasher, BuildHasherDefault, Hasher},
    ops::ControlFlow,
    sync::atomic::{AtomicPtr, AtomicU64, Ordering},
};
use eonix_sync::LazyLock;
use pointers::BorrowedArc;
use posix_types::{namei::RenameFlags, open::OpenFlags, result::PosixError, stat::StatX};

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
    parent: RCUPointer<Dentry>,
    name: RCUPointer<Arc<[u8]>>,
    hash: AtomicU64,

    // Used by the dentry cache
    prev: AtomicPtr<Dentry>,
    next: AtomicPtr<Dentry>,

    // RCU Mutable
    data: RCUPointer<DentryData>,
}

pub(super) static DROOT: LazyLock<Arc<Dentry>> = LazyLock::new(|| {
    let root = Arc::new(Dentry {
        parent: RCUPointer::empty(),
        name: RCUPointer::new(Arc::new(Arc::from(&b"[root]"[..]))),
        hash: AtomicU64::new(0),
        prev: AtomicPtr::default(),
        next: AtomicPtr::default(),
        data: RCUPointer::empty(),
    });

    unsafe {
        root.parent.swap(Some(root.clone()));
    }

    root.rehash();

    root
});

impl fmt::Debug for Dentry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dentry")
            .field("name", &String::from_utf8_lossy(&self.name()))
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
    fn is_hashed(&self) -> bool {
        self.prev.load(Ordering::Relaxed) != core::ptr::null_mut()
    }

    fn rehash(&self) {
        assert!(
            !self.is_hashed(),
            "`rehash()` called on some already hashed dentry"
        );

        let builder: BuildHasherDefault<KernelHasher> = Default::default();
        let mut hasher = builder.build_hasher();

        hasher.write_usize(self.parent_addr() as usize);
        hasher.write(&self.name());
        let hash = hasher.finish();

        self.hash.store(hash, Ordering::Relaxed);
    }

    fn find(self: &Arc<Self>, name: &[u8]) -> KResult<Arc<Self>> {
        let data = self.data.load();
        let data = data.as_ref().ok_or(ENOENT)?;

        if data.flags & D_DIRECTORY == 0 {
            return Err(ENOTDIR);
        }

        match name {
            b"." => Ok(self.clone()),
            b".." => Ok(self.parent().clone()),
            _ => {
                let dentry = Dentry::create(self.clone(), name);

                if let Some(found) = dcache::d_find_fast(&dentry) {
                    unsafe {
                        // SAFETY: This is safe because the dentry is never shared with
                        //         others so we can drop them safely.
                        let _ = dentry.name.swap(None);
                        let _ = dentry.parent.swap(None);
                    }

                    return Ok(found);
                }

                dcache::d_try_revalidate(&dentry);
                dcache::d_add(dentry.clone());

                Ok(dentry)
            }
        }
    }
}

impl Dentry {
    pub fn create(parent: Arc<Dentry>, name: &[u8]) -> Arc<Self> {
        let val = Arc::new(Self {
            parent: RCUPointer::new(parent),
            name: RCUPointer::new(Arc::new(Arc::from(name))),
            hash: AtomicU64::new(0),
            prev: AtomicPtr::default(),
            next: AtomicPtr::default(),
            data: RCUPointer::empty(),
        });

        val.rehash();
        val
    }

    /// Check the equality of two denties inside the same dentry cache hash group
    /// where `other` is identified by `hash`, `parent` and `name`
    ///
    fn hash_eq(&self, other: &Self) -> bool {
        self.hash.load(Ordering::Relaxed) == other.hash.load(Ordering::Relaxed)
            && self.parent_addr() == other.parent_addr()
            && &***self.name() == &***other.name()
    }

    pub fn name(&self) -> RCUReadGuard<BorrowedArc<Arc<[u8]>>> {
        self.name.load().expect("Dentry has no name")
    }

    pub fn get_name(&self) -> Arc<[u8]> {
        (***self.name()).clone()
    }

    pub fn parent<'a>(&self) -> RCUReadGuard<'a, BorrowedArc<Dentry>> {
        self.parent.load().expect("Dentry has no parent")
    }

    pub fn parent_addr(&self) -> *const Self {
        self.parent
            .load()
            .map_or(core::ptr::null(), |parent| Arc::as_ptr(&parent))
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

    pub fn open_check(self: &Arc<Self>, flags: OpenFlags, mode: Mode) -> KResult<()> {
        let data = self.data.load();

        if data.is_some() {
            if flags.contains(OpenFlags::O_CREAT | OpenFlags::O_EXCL) {
                Err(EEXIST)
            } else {
                Ok(())
            }
        } else {
            if !flags.contains(OpenFlags::O_CREAT) {
                return Err(ENOENT);
            }

            let parent = self.parent().get_inode()?;
            parent.creat(self, mode)
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

                let dentry =
                    Self::open_recursive(context, &dentry.parent(), path, true, nrecur + 1)?;

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
                        let parent = cwd.parent().clone();
                        cwd = Self::resolve_directory(context, parent, nrecur)?;
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

                    let parent = cwd.parent().clone();
                    cwd = Self::open_recursive(context, &parent, path, true, nrecur + 1)?;
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
        let locked_parent = self.parent();

        let path = {
            let mut path = vec![];

            let mut parent = locked_parent.borrow();
            let mut dentry = BorrowedArc::new(self);

            while Arc::as_ptr(&dentry) != Arc::as_ptr(&context.fsroot) {
                if path.len() > 32 {
                    return Err(ELOOP);
                }

                path.push(dentry.name().clone());
                dentry = parent;
                parent = dentry.parent.load_protected(&locked_parent).unwrap();
            }

            path
        };

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
    pub fn size(&self) -> usize {
        self.get_inode().unwrap().file_size()
    }

    pub fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        let inode = self.get_inode()?;

        // Safety: Changing mode alone will have no effect on the file's contents
        match inode.mode.load().format() {
            Mode::DIR => Err(EISDIR),
            Mode::REG => inode.read(buffer, offset),
            Mode::BLK => {
                let device = BlockDevice::get(inode.devid()?)?;
                Ok(device.read_some(offset, buffer)?.allow_partial())
            }
            Mode::CHR => {
                let device = CharDevice::get(inode.devid()?).ok_or(EPERM)?;
                device.read(buffer)
            }
            _ => Err(EINVAL),
        }
    }

    pub fn write(&self, stream: &mut dyn Stream, offset: WriteOffset) -> KResult<usize> {
        let inode = self.get_inode()?;
        // Safety: Changing mode alone will have no effect on the file's contents
        match inode.mode.load().format() {
            Mode::DIR => Err(EISDIR),
            Mode::REG => inode.write(stream, offset),
            Mode::BLK => Err(EINVAL), // TODO
            Mode::CHR => CharDevice::get(inode.devid()?).ok_or(EPERM)?.write(stream),
            _ => Err(EINVAL),
        }
    }

    pub fn readdir<F>(&self, offset: usize, mut callback: F) -> KResult<usize>
    where
        F: FnMut(&[u8], Ino) -> KResult<ControlFlow<(), ()>>,
    {
        let dir = self.get_inode()?;
        dir.do_readdir(offset, &mut callback)
    }

    pub fn mkdir(&self, mode: Mode) -> KResult<()> {
        if self.get_inode().is_ok() {
            Err(EEXIST)
        } else {
            let dir = self.parent().get_inode()?;
            dir.mkdir(self, mode)
        }
    }

    pub fn statx(&self, stat: &mut StatX, mask: u32) -> KResult<()> {
        self.get_inode()?.statx(stat, mask)
    }

    pub fn truncate(&self, size: usize) -> KResult<()> {
        self.get_inode()?.truncate(size)
    }

    pub fn unlink(self: &Arc<Self>) -> KResult<()> {
        if self.get_inode().is_err() {
            Err(ENOENT)
        } else {
            let dir = self.parent().get_inode()?;
            dir.unlink(self)
        }
    }

    pub fn symlink(self: &Arc<Self>, link: &[u8]) -> KResult<()> {
        if self.get_inode().is_ok() {
            Err(EEXIST)
        } else {
            let dir = self.parent().get_inode()?;
            dir.symlink(self, link)
        }
    }

    pub fn linkat(self: &Arc<Self>, inode: Arc<dyn Inode>) -> KResult<()> {
        if self.get_inode().is_ok() {
            Err(EEXIST)
        } else {
            let dir = self.parent().get_inode()?;
            dir.linkat(self, inode)
        }
    }

    pub fn readlink(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        self.get_inode()?.readlink(buffer)
    }

    pub fn mknod(&self, mode: Mode, devid: DevId) -> KResult<()> {
        if self.get_inode().is_ok() {
            Err(EEXIST)
        } else {
            let dir = self.parent().get_inode()?;
            dir.mknod(self, mode, devid)
        }
    }

    pub fn chmod(&self, mode: Mode) -> KResult<()> {
        self.get_inode()?.chmod(mode)
    }

    pub fn chown(&self, uid: u32, gid: u32) -> KResult<()> {
        self.get_inode()?.chown(uid, gid)
    }

    pub fn rename(self: &Arc<Self>, new: &Arc<Self>, flags: RenameFlags) -> KResult<()> {
        if Arc::ptr_eq(self, new) {
            return Ok(());
        }

        let old_parent = self.parent().get_inode()?;
        let new_parent = new.parent().get_inode()?;

        // If the two dentries are not in the same filesystem, return EXDEV.
        if !Weak::ptr_eq(&old_parent.vfs, &new_parent.vfs) {
            Err(PosixError::EXDEV)?;
        }

        let vfs = old_parent.vfs.upgrade().ok_or(EIO)?;

        let rename_data = RenameData {
            old_dentry: self,
            new_dentry: new,
            new_parent,
            vfs,
            is_exchange: flags.contains(RenameFlags::RENAME_EXCHANGE),
            no_replace: flags.contains(RenameFlags::RENAME_NOREPLACE),
        };

        // Delegate to the parent directory's rename implementation
        old_parent.rename(rename_data)
    }
}
