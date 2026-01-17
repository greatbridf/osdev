pub mod dcache;
mod walk;

use alloc::sync::Arc;
use core::cell::UnsafeCell;
use core::fmt;
use core::hash::{BuildHasher, BuildHasherDefault, Hasher};
use core::sync::atomic::{AtomicPtr, AtomicU64, AtomicU8, Ordering};

use arcref::AsArcRef;
use eonix_sync::LazyLock;
use pointers::BorrowedArc;
use posix_types::namei::RenameFlags;
use posix_types::open::OpenFlags;
use posix_types::result::PosixError;
use posix_types::stat::StatX;

use super::inode::{Ino, InodeUse, RenameData, WriteOffset};
use super::types::{DeviceId, Format, Mode, Permission};
use super::FsContext;
use crate::hash::KernelHasher;
use crate::io::{Buffer, Stream};
use crate::kernel::block::BlockDevice;
use crate::kernel::constants::{EEXIST, EINVAL, EISDIR, ELOOP, ENOENT, EPERM, ERANGE};
use crate::kernel::CharDevice;
use crate::path::Path;
use crate::prelude::*;
use crate::rcu::{rcu_read_lock, RCUNode, RCUPointer, RCUReadGuard};

// TODO: Implement slab reclaim
#[allow(unused)]
const D_INVALID: u8 = 0;
const D_REGULAR: u8 = 1;
const D_DIRECTORY: u8 = 2;
const D_SYMLINK: u8 = 3;

#[derive(Debug, PartialEq, Eq)]
enum DentryKind {
    Regular = D_REGULAR as isize,
    Directory = D_DIRECTORY as isize,
    Symlink = D_SYMLINK as isize,
}

/// The [`Inode`] associated with a [`Dentry`].
///
/// We could assign an inode to a negative dentry exactly once when the dentry
/// is invalid and we create a file or directory on it, or the dentry is brought
/// to the dcache by [lookup()].
///
/// This guarantees that as long as we acquire a non-invalid from [`Self::kind`],
/// we are synced with the writer and can safely read the [`Self::inode`] field
/// without reading torn data.
///
/// [lookup()]: crate::kernel::vfs::inode::InodeDirOps::lookup
struct AssociatedInode {
    kind: UnsafeCell<Option<DentryKind>>,
    inode: UnsafeCell<Option<InodeUse>>,
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

    inode: AssociatedInode,
}

pub(super) static DROOT: LazyLock<Arc<Dentry>> = LazyLock::new(|| {
    let root = Arc::new(Dentry {
        parent: RCUPointer::empty(),
        name: RCUPointer::new(Arc::new(Arc::from(&b"[root]"[..]))),
        hash: AtomicU64::new(0),
        prev: AtomicPtr::default(),
        next: AtomicPtr::default(),
        inode: AssociatedInode::new(),
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
}

impl Dentry {
    pub fn create(parent: Arc<Dentry>, name: &[u8]) -> Arc<Self> {
        // TODO!!!: don't acquire our parent's refcount here...

        let val = Arc::new(Self {
            parent: RCUPointer::new(parent),
            name: RCUPointer::new(Arc::new(Arc::from(name))),
            hash: AtomicU64::new(0),
            prev: AtomicPtr::default(),
            next: AtomicPtr::default(),
            inode: AssociatedInode::new(),
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

    pub fn name(&self) -> RCUReadGuard<'_, BorrowedArc<'_, Arc<[u8]>>> {
        self.name.load().expect("Dentry has no name")
    }

    pub fn get_name(&self) -> Arc<[u8]> {
        (***self.name()).clone()
    }

    pub fn parent<'a>(&self) -> RCUReadGuard<'a, BorrowedArc<'_, Dentry>> {
        self.parent.load().expect("Dentry has no parent")
    }

    pub fn parent_addr(&self) -> *const Self {
        self.parent
            .load()
            .map_or(core::ptr::null(), |parent| Arc::as_ptr(&parent))
    }

    pub fn fill(&self, file: InodeUse) {
        self.inode.store(file);
    }

    pub fn inode(&self) -> Option<InodeUse> {
        self.inode.load().map(|(_, inode)| inode.clone())
    }

    pub fn get_inode(&self) -> KResult<InodeUse> {
        self.inode().ok_or(ENOENT)
    }

    pub fn is_directory(&self) -> bool {
        self.inode
            .load()
            .map_or(false, |(kind, _)| kind == DentryKind::Directory)
    }

    pub fn is_valid(&self) -> bool {
        self.inode.load().is_some()
    }

    pub async fn open_check(self: &Arc<Self>, flags: OpenFlags, perm: Permission) -> KResult<()> {
        match self.inode.load() {
            Some(_) => {
                if flags.contains(OpenFlags::O_CREAT | OpenFlags::O_EXCL) {
                    Err(EEXIST)
                } else {
                    Ok(())
                }
            }
            None => {
                if !flags.contains(OpenFlags::O_CREAT) {
                    return Err(ENOENT);
                }

                let parent = self.parent().get_inode()?;
                parent.create(self, perm).await
            }
        }
    }
}

impl Dentry {
    pub async fn open(
        context: &FsContext,
        path: &Path,
        follow_symlinks: bool,
    ) -> KResult<Arc<Self>> {
        let cwd = context.cwd.lock().clone();
        Self::open_at(context, &cwd, path, follow_symlinks).await
    }

    pub async fn open_at(
        context: &FsContext,
        at: &Arc<Self>,
        path: &Path,
        follow_symlinks: bool,
    ) -> KResult<Arc<Self>> {
        let mut found = context.start_recursive_walk(at, path).await?;

        if !follow_symlinks {
            return Ok(found);
        }

        loop {
            match found.inode.load() {
                Some((DentryKind::Symlink, inode)) => {
                    found = context.follow_symlink(found.aref(), inode, 0).await?;
                }
                _ => return Ok(found),
            }
        }
    }

    pub fn get_path(self: &Arc<Self>, context: &FsContext, buffer: &mut dyn Buffer) -> KResult<()> {
        let rcu_read = rcu_read_lock();

        let mut path = vec![];

        let mut current = self.aref();
        let mut parent = self.parent.dereference(&rcu_read).unwrap();

        while !current.ptr_eq_arc(&context.fsroot) {
            if path.len() > 32 {
                return Err(ELOOP);
            }

            path.push(current.name.dereference(&rcu_read).unwrap());
            current = parent;
            parent = current.parent.dereference(&rcu_read).unwrap();
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
    pub async fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        let inode = self.get_inode()?;

        // Safety: Changing mode alone will have no effect on the file's contents
        match inode.format {
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
        match inode.format {
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
        self.get_inode()?.chmod(mode.perm()).await
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

impl DentryKind {
    fn into_raw(self) -> u8 {
        unsafe { core::mem::transmute(self) }
    }

    fn from_raw(raw: u8) -> Option<Self> {
        unsafe { core::mem::transmute(raw) }
    }

    fn as_atomic(me: &UnsafeCell<Option<Self>>) -> &AtomicU8 {
        unsafe { AtomicU8::from_ptr(me.get().cast()) }
    }

    fn atomic_acq(me: &UnsafeCell<Option<Self>>) -> Option<Self> {
        Self::from_raw(Self::as_atomic(me).load(Ordering::Acquire))
    }

    fn atomic_swap_acqrel(me: &UnsafeCell<Option<Self>>, kind: Option<Self>) -> Option<Self> {
        Self::from_raw(Self::as_atomic(me).swap(kind.map_or(0, Self::into_raw), Ordering::AcqRel))
    }
}

impl AssociatedInode {
    fn new() -> Self {
        Self {
            inode: UnsafeCell::new(None),
            kind: UnsafeCell::new(None),
        }
    }

    fn store(&self, inode: InodeUse) {
        let kind = match inode.format {
            Format::REG | Format::BLK | Format::CHR => DentryKind::Regular,
            Format::DIR => DentryKind::Directory,
            Format::LNK => DentryKind::Symlink,
        };

        unsafe {
            // SAFETY: We should be the first and only one to store the inode as
            //         is checked below. All other readers reading non-invalid
            //         kind will see the fully written inode.
            self.inode.get().write(Some(inode));
        }

        assert_eq!(
            DentryKind::atomic_swap_acqrel(&self.kind, Some(kind)),
            None,
            "Dentry can only be stored once."
        );
    }

    fn kind(&self) -> Option<DentryKind> {
        DentryKind::atomic_acq(&self.kind)
    }

    fn load(&self) -> Option<(DentryKind, &InodeUse)> {
        self.kind().map(|kind| unsafe {
            let inode = (&*self.inode.get())
                .as_ref()
                .expect("Dentry with non-invalid kind has no inode");
            (kind, inode)
        })
    }
}

unsafe impl Send for AssociatedInode {}
unsafe impl Sync for AssociatedInode {}
