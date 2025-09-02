pub mod dcache;

use super::{
    inode::{Ino, Inode, InodeUse, RenameData, WriteOffset},
    types::{DeviceId, Format, Mode, Permission},
    FsContext,
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
    kernel::constants::{EEXIST, EINVAL, EISDIR, ELOOP, ENOENT, ENOTDIR, EPERM, ERANGE},
};
use alloc::sync::Arc;
use core::{
    fmt,
    future::Future,
    hash::{BuildHasher, BuildHasherDefault, Hasher},
    pin::Pin,
    sync::atomic::{AtomicPtr, AtomicU64, Ordering},
};
use eonix_sync::LazyLock;
use pointers::BorrowedArc;
use posix_types::{namei::RenameFlags, open::OpenFlags, result::PosixError, stat::StatX};

#[derive(PartialEq, Eq)]
enum DentryKind {
    Regular,
    Directory,
    Symlink,
    Mountpoint,
}

struct DentryData {
    inode: InodeUse<dyn Inode>,
    kind: DentryKind,
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

    async fn find(self: &Arc<Self>, name: &[u8]) -> KResult<Arc<Self>> {
        let data = self.data.load();
        let data = data.as_ref().ok_or(ENOENT)?;

        if data.kind != DentryKind::Directory {
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

                let _ = dcache::d_try_revalidate(&dentry).await;
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

    fn save(&self, inode: InodeUse<dyn Inode>, kind: DentryKind) {
        let new = DentryData { inode, kind };

        // TODO!!!: We don't actually need to use `RCUPointer` here
        // Safety: this function may only be called from `create`-like functions which requires the
        // superblock's write locks to be held, so only one creation can happen at a time and we
        // can't get a reference to the old data.
        let old = unsafe { self.data.swap(Some(Arc::new(new))) };
        assert!(old.is_none());
    }

    pub fn fill(&self, file: InodeUse<dyn Inode>) {
        match file.format() {
            Format::REG | Format::BLK | Format::CHR => self.save(file, DentryKind::Regular),
            Format::DIR => self.save(file, DentryKind::Directory),
            Format::LNK => self.save(file, DentryKind::Symlink),
        }
    }

    pub fn inode(&self) -> Option<InodeUse<dyn Inode>> {
        self.data.load().as_ref().map(|data| data.inode.clone())
    }

    pub fn get_inode(&self) -> KResult<InodeUse<dyn Inode>> {
        self.inode().ok_or(ENOENT)
    }

    pub fn is_directory(&self) -> bool {
        let data = self.data.load();
        data.as_ref()
            .map_or(false, |data| data.kind == DentryKind::Directory)
    }

    pub fn is_valid(&self) -> bool {
        self.data.load().is_some()
    }

    pub async fn open_check(self: &Arc<Self>, flags: OpenFlags, perm: Permission) -> KResult<()> {
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
            parent.create(self, perm).await
        }
    }
}

impl Dentry {
    fn resolve_directory(
        context: &FsContext,
        dentry: Arc<Self>,
        nrecur: u32,
    ) -> Pin<Box<impl Future<Output = KResult<Arc<Self>>> + use<'_>>> {
        Box::pin(async move {
            if nrecur >= 16 {
                return Err(ELOOP);
            }

            let data = dentry.data.load();
            let data = data.as_ref().ok_or(ENOENT)?;

            match data.kind {
                DentryKind::Regular => Err(ENOTDIR),
                DentryKind::Directory => Ok(dentry),
                DentryKind::Symlink => {
                    let mut buffer = [0u8; 256];
                    let mut buffer = ByteBuffer::new(&mut buffer);

                    data.inode.readlink(&mut buffer).await?;
                    let path = Path::new(buffer.data())?;

                    let dentry =
                        Self::open_recursive(context, &dentry.parent(), path, true, nrecur + 1)
                            .await?;

                    Self::resolve_directory(context, dentry, nrecur + 1).await
                }
                _ => panic!("Invalid dentry flags"),
            }
        })
    }

    pub fn open_recursive<'r, 'a: 'r, 'b: 'r, 'c: 'r>(
        context: &'a FsContext,
        cwd: &'b Arc<Self>,
        path: Path<'c>,
        follow: bool,
        nrecur: u32,
    ) -> Pin<Box<impl Future<Output = KResult<Arc<Self>>> + 'r>> {
        Box::pin(async move {
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

                cwd = Self::resolve_directory(context, cwd, nrecur).await?;

                match item {
                    PathComponent::TrailingEmpty | PathComponent::Current => {} // pass
                    PathComponent::Parent => {
                        if !cwd.hash_eq(&context.fsroot) {
                            let parent = cwd.parent().clone();
                            cwd = Self::resolve_directory(context, parent, nrecur).await?;
                        }
                        continue;
                    }
                    PathComponent::Name(name) => {
                        cwd = cwd.find(name).await?;
                    }
                }
            }

            if follow {
                let data = cwd.data.load();

                if let Some(data) = data.as_ref() {
                    if data.kind == DentryKind::Symlink {
                        let data = cwd.data.load();
                        let data = data.as_ref().unwrap();
                        let mut buffer = [0u8; 256];
                        let mut buffer = ByteBuffer::new(&mut buffer);

                        data.inode.readlink(&mut buffer).await?;
                        let path = Path::new(buffer.data())?;

                        let parent = cwd.parent().clone();
                        cwd =
                            Self::open_recursive(context, &parent, path, true, nrecur + 1).await?;
                    }
                }
            }

            Ok(cwd)
        })
    }

    pub async fn open(
        context: &FsContext,
        path: Path<'_>,
        follow_symlinks: bool,
    ) -> KResult<Arc<Self>> {
        let cwd = context.cwd.lock().clone();
        Dentry::open_recursive(context, &cwd, path, follow_symlinks, 0).await
    }

    pub async fn open_at(
        context: &FsContext,
        at: &Arc<Self>,
        path: Path<'_>,
        follow_symlinks: bool,
    ) -> KResult<Arc<Self>> {
        Dentry::open_recursive(context, at, path, follow_symlinks, 0).await
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
    pub async fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        let inode = self.get_inode()?;

        // Safety: Changing mode alone will have no effect on the file's contents
        match inode.format() {
            Format::DIR => Err(EISDIR),
            Format::REG => inode.read(buffer, offset).await,
            Format::BLK => {
                let device = BlockDevice::get(inode.devid()?)?;
                Ok(device.read_some(offset, buffer).await?.allow_partial())
            }
            Format::CHR => {
                let device = CharDevice::get(inode.devid()?).ok_or(EPERM)?;
                device.read(buffer)
            }
            _ => Err(EINVAL),
        }
    }

    pub async fn write(&self, stream: &mut dyn Stream, offset: WriteOffset<'_>) -> KResult<usize> {
        let inode = self.get_inode()?;
        // Safety: Changing mode alone will have no effect on the file's contents
        match inode.format() {
            Format::DIR => Err(EISDIR),
            Format::REG => inode.write(stream, offset).await,
            Format::BLK => Err(EINVAL), // TODO
            Format::CHR => CharDevice::get(inode.devid()?).ok_or(EPERM)?.write(stream),
            _ => Err(EINVAL),
        }
    }

    pub async fn readdir<F>(&self, offset: usize, mut for_each_entry: F) -> KResult<KResult<usize>>
    where
        F: FnMut(&[u8], Ino) -> KResult<bool> + Send,
    {
        let dir = self.get_inode()?;
        dir.readdir(offset, &mut for_each_entry).await
    }

    pub async fn mkdir(&self, perm: Permission) -> KResult<()> {
        if self.get_inode().is_ok() {
            Err(EEXIST)
        } else {
            let dir = self.parent().get_inode()?;
            dir.mkdir(self, perm).await
        }
    }

    pub fn statx(&self, stat: &mut StatX, mask: u32) -> KResult<()> {
        self.get_inode()?.statx(stat, mask)
    }

    pub async fn truncate(&self, size: usize) -> KResult<()> {
        self.get_inode()?.truncate(size).await
    }

    pub async fn unlink(self: &Arc<Self>) -> KResult<()> {
        if self.get_inode().is_err() {
            Err(ENOENT)
        } else {
            let dir = self.parent().get_inode()?;
            dir.unlink(self).await
        }
    }

    pub async fn symlink(self: &Arc<Self>, link: &[u8]) -> KResult<()> {
        if self.get_inode().is_ok() {
            Err(EEXIST)
        } else {
            let dir = self.parent().get_inode()?;
            dir.symlink(self, link).await
        }
    }

    pub async fn readlink(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        self.get_inode()?.readlink(buffer).await
    }

    pub async fn mknod(&self, mode: Mode, devid: DeviceId) -> KResult<()> {
        if self.get_inode().is_ok() {
            Err(EEXIST)
        } else {
            let dir = self.parent().get_inode()?;
            dir.mknod(self, mode, devid).await
        }
    }

    pub async fn chmod(&self, mode: Mode) -> KResult<()> {
        self.get_inode()?.chmod(mode).await
    }

    pub async fn chown(&self, uid: u32, gid: u32) -> KResult<()> {
        self.get_inode()?.chown(uid, gid).await
    }

    pub async fn rename(self: &Arc<Self>, new: &Arc<Self>, flags: RenameFlags) -> KResult<()> {
        if Arc::ptr_eq(self, new) {
            return Ok(());
        }

        let old_parent = self.parent().get_inode()?;
        let new_parent = new.parent().get_inode()?;

        // If the two dentries are not in the same filesystem, return EXDEV.
        if old_parent.sbref().eq(&new_parent.sbref()) {
            Err(PosixError::EXDEV)?;
        }

        let rename_data = RenameData {
            old_dentry: self,
            new_dentry: new,
            new_parent,
            is_exchange: flags.contains(RenameFlags::RENAME_EXCHANGE),
            no_replace: flags.contains(RenameFlags::RENAME_NOREPLACE),
        };

        // Delegate to the parent directory's rename implementation
        old_parent.rename(rename_data).await
    }
}
