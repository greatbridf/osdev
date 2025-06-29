use crate::io::Stream;
use crate::kernel::constants::{EEXIST, EINVAL, EIO, EISDIR, ENOENT, ENOSYS, ENOTDIR};
use crate::kernel::mem::{CachePage, PageCache, PageCacheBackend};
use crate::kernel::timer::Instant;
use crate::kernel::vfs::inode::InodeData;
use crate::kernel::vfs::inode::RenameData;
use crate::{
    io::Buffer,
    kernel::constants::{S_IFBLK, S_IFCHR, S_IFDIR, S_IFLNK, S_IFREG},
    kernel::vfs::{
        dentry::{dcache, Dentry},
        inode::{define_struct_inode, AtomicIno, Ino, Inode, Mode, WriteOffset},
        mount::{register_filesystem, Mount, MountCreator, MS_RDONLY},
        s_isblk, s_ischr,
        vfs::Vfs,
        DevId,
    },
    prelude::*,
};
use alloc::sync::{Arc, Weak};
use core::fmt::Debug;
use core::{ops::ControlFlow, sync::atomic::Ordering};
use eonix_mm::paging::PAGE_SIZE;
use eonix_runtime::task::Task;
use eonix_sync::{AsProof as _, AsProofMut as _, Locked, Mutex, ProofMut};
use itertools::Itertools;

fn acquire(vfs: &Weak<dyn Vfs>) -> KResult<Arc<dyn Vfs>> {
    vfs.upgrade().ok_or(EIO)
}

fn astmp(vfs: &Arc<dyn Vfs>) -> &TmpFs {
    vfs.as_any()
        .downcast_ref::<TmpFs>()
        .expect("corrupted tmpfs data structure")
}

define_struct_inode! {
    struct NodeInode {
        devid: DevId,
    }
}

impl NodeInode {
    fn new(ino: Ino, vfs: Weak<dyn Vfs>, mode: Mode, devid: DevId) -> Arc<Self> {
        Self::new_locked(ino, vfs, |inode, _| unsafe {
            addr_of_mut_field!(inode, devid).write(devid);

            addr_of_mut_field!(&mut *inode, mode).write(mode.into());
            addr_of_mut_field!(&mut *inode, nlink).write(1.into());
            addr_of_mut_field!(&mut *inode, ctime).write(Spin::new(Instant::now()));
            addr_of_mut_field!(&mut *inode, mtime).write(Spin::new(Instant::now()));
            addr_of_mut_field!(&mut *inode, atime).write(Spin::new(Instant::now()));
        })
    }
}

impl Inode for NodeInode {
    fn devid(&self) -> KResult<DevId> {
        Ok(self.devid)
    }
}

define_struct_inode! {
    pub(super) struct DirectoryInode {
        entries: Locked<Vec<(Arc<[u8]>, Ino)>, ()>,
    }
}

impl DirectoryInode {
    fn new(ino: Ino, vfs: Weak<dyn Vfs>, mode: Mode) -> Arc<Self> {
        Self::new_locked(ino, vfs, |inode, rwsem| unsafe {
            addr_of_mut_field!(inode, entries)
                .write(Locked::new(vec![(Arc::from(b".".as_slice()), ino)], rwsem));

            addr_of_mut_field!(&mut *inode, size).write(1.into());
            addr_of_mut_field!(&mut *inode, mode).write((S_IFDIR | (mode & 0o777)).into());
            addr_of_mut_field!(&mut *inode, nlink).write(1.into()); // link from `.` to itself
            addr_of_mut_field!(&mut *inode, ctime).write(Spin::new(Instant::now()));
            addr_of_mut_field!(&mut *inode, mtime).write(Spin::new(Instant::now()));
            addr_of_mut_field!(&mut *inode, atime).write(Spin::new(Instant::now()));
        })
    }

    fn link(&self, name: Arc<[u8]>, file: &dyn Inode, dlock: ProofMut<'_, ()>) {
        let now = Instant::now();

        // SAFETY: Only `unlink` will do something based on `nlink` count
        //         No need to synchronize here
        file.nlink.fetch_add(1, Ordering::Relaxed);
        *self.ctime.lock() = now;

        // SAFETY: `rwsem` has done the synchronization
        self.size.fetch_add(1, Ordering::Relaxed);
        *self.mtime.lock() = now;

        self.entries.access_mut(dlock).push((name, file.ino));
    }

    fn do_unlink(
        &self,
        file: &Arc<dyn Inode>,
        filename: &[u8],
        entries: &mut Vec<(Arc<[u8]>, Ino)>,
        now: Instant,
        decrease_size: bool,
        _dir_lock: ProofMut<()>,
        _file_lock: ProofMut<()>,
    ) -> KResult<()> {
        // SAFETY: `file_lock` has done the synchronization
        if file.mode.load(Ordering::Relaxed) & S_IFDIR != 0 {
            return Err(EISDIR);
        }

        entries.retain(|(name, ino)| *ino != file.ino || name.as_ref() != filename);

        if decrease_size {
            // SAFETY: `dir_lock` has done the synchronization
            self.size.fetch_sub(1, Ordering::Relaxed);
        }

        *self.mtime.lock() = now;

        // The last reference to the inode is held by some dentry
        // and will be released when the dentry is released

        // SAFETY: `file_lock` has done the synchronization
        file.nlink.fetch_sub(1, Ordering::Relaxed);
        *file.ctime.lock() = now;

        Ok(())
    }
}

impl Inode for DirectoryInode {
    fn do_readdir(
        &self,
        offset: usize,
        callback: &mut dyn FnMut(&[u8], Ino) -> KResult<ControlFlow<(), ()>>,
    ) -> KResult<usize> {
        let lock = Task::block_on(self.rwsem.read());
        self.entries
            .access(lock.prove())
            .iter()
            .skip(offset)
            .map(|(name, ino)| callback(&name, *ino))
            .take_while(|result| result.map_or(true, |flow| flow.is_continue()))
            .take_while_inclusive(|result| result.is_ok())
            .fold_ok(0, |acc, _| acc + 1)
    }

    fn creat(&self, at: &Arc<Dentry>, mode: Mode) -> KResult<()> {
        let vfs = acquire(&self.vfs)?;
        let vfs = astmp(&vfs);

        let rwsem = Task::block_on(self.rwsem.write());

        let ino = vfs.assign_ino();
        let file = FileInode::new(ino, self.vfs.clone(), 0, mode);

        self.link(at.get_name(), file.as_ref(), rwsem.prove_mut());
        at.save_reg(file)
    }

    fn mknod(&self, at: &Dentry, mode: Mode, dev: DevId) -> KResult<()> {
        if !s_ischr(mode) && !s_isblk(mode) {
            return Err(EINVAL);
        }

        let vfs = acquire(&self.vfs)?;
        let vfs = astmp(&vfs);

        let rwsem = Task::block_on(self.rwsem.write());

        let ino = vfs.assign_ino();
        let file = NodeInode::new(
            ino,
            self.vfs.clone(),
            mode & (0o777 | S_IFBLK | S_IFCHR),
            dev,
        );

        self.link(at.get_name(), file.as_ref(), rwsem.prove_mut());
        at.save_reg(file)
    }

    fn symlink(&self, at: &Arc<Dentry>, target: &[u8]) -> KResult<()> {
        let vfs = acquire(&self.vfs)?;
        let vfs = astmp(&vfs);

        let rwsem = Task::block_on(self.rwsem.write());

        let ino = vfs.assign_ino();
        let file = SymlinkInode::new(ino, self.vfs.clone(), target.into());

        self.link(at.get_name(), file.as_ref(), rwsem.prove_mut());
        at.save_symlink(file)
    }

    fn mkdir(&self, at: &Dentry, mode: Mode) -> KResult<()> {
        let vfs = acquire(&self.vfs)?;
        let vfs = astmp(&vfs);

        let rwsem = Task::block_on(self.rwsem.write());

        let ino = vfs.assign_ino();
        let newdir = DirectoryInode::new(ino, self.vfs.clone(), mode);

        self.link(at.get_name(), newdir.as_ref(), rwsem.prove_mut());
        at.save_dir(newdir)
    }

    fn unlink(&self, at: &Arc<Dentry>) -> KResult<()> {
        let _vfs = acquire(&self.vfs)?;

        let dir_lock = Task::block_on(self.rwsem.write());

        let file = at.get_inode()?;
        let filename = at.get_name();
        let file_lock = Task::block_on(file.rwsem.write());

        let entries = self.entries.access_mut(dir_lock.prove_mut());

        self.do_unlink(
            &file,
            &filename,
            entries,
            Instant::now(),
            true,
            dir_lock.prove_mut(),
            file_lock.prove_mut(),
        )?;

        // Remove the dentry from the dentry cache immediately
        // so later lookup will fail with ENOENT
        dcache::d_remove(at);

        Ok(())
    }

    fn chmod(&self, mode: Mode) -> KResult<()> {
        let _vfs = acquire(&self.vfs)?;
        let _lock = Task::block_on(self.rwsem.write());

        // SAFETY: `rwsem` has done the synchronization
        let old = self.mode.load(Ordering::Relaxed);
        self.mode
            .store((old & !0o777) | (mode & 0o777), Ordering::Relaxed);
        *self.ctime.lock() = Instant::now();

        Ok(())
    }

    fn rename(&self, rename_data: RenameData) -> KResult<()> {
        let RenameData {
            old_dentry,
            new_dentry,
            new_parent,
            is_exchange,
            no_replace,
            vfs,
        } = rename_data;

        if is_exchange {
            println_warn!("TmpFs does not support exchange rename for now");
            return Err(ENOSYS);
        }

        let vfs = vfs
            .as_any()
            .downcast_ref::<TmpFs>()
            .expect("vfs must be a TmpFs");

        let _rename_lock = Task::block_on(vfs.rename_lock.lock());

        let old_file = old_dentry.get_inode()?;
        let new_file = new_dentry.get_inode();

        if no_replace && new_file.is_ok() {
            return Err(EEXIST);
        }

        let same_parent = Arc::as_ptr(&new_parent) == &raw const *self;
        if same_parent {
            // Same directory rename
            // Remove from old location and add to new location
            let parent_lock = Task::block_on(self.rwsem.write());
            let entries = self.entries.access_mut(parent_lock.prove_mut());

            fn rename_old(
                old_entry: &mut (Arc<[u8]>, Ino),
                old_file: &Arc<dyn Inode + 'static>,
                new_dentry: &Arc<Dentry>,
                now: Instant,
            ) {
                let (name, _) = old_entry;
                *name = new_dentry.get_name();
                *old_file.ctime.lock() = now;
            }

            let old_ino = old_file.ino;
            let new_ino = new_file.as_ref().ok().map(|f| f.ino);
            let old_name = old_dentry.get_name();
            let new_name = new_dentry.get_name();

            // Find the old and new entries in the directory after we've locked the directory.
            let indices =
                entries
                    .iter()
                    .enumerate()
                    .fold([None, None], |[old, new], (idx, (name, ino))| {
                        if Some(*ino) == new_ino && *name == new_name {
                            [old, Some(idx)]
                        } else if *ino == old_ino && *name == old_name {
                            [Some(idx), new]
                        } else {
                            [old, new]
                        }
                    });

            let (old_entry_idx, new_entry_idx) = match indices {
                [None, ..] => return Err(ENOENT),
                [Some(old_idx), new_idx] => (old_idx, new_idx),
            };

            let now = Instant::now();

            if let Some(new_idx) = new_entry_idx {
                // Replace existing file (i.e. rename the old and unlink the new)
                let new_file = new_file.unwrap();
                let _new_file_lock = Task::block_on(new_file.rwsem.write());

                // SAFETY: `new_file_lock` has done the synchronization
                if new_file.mode.load(Ordering::Relaxed) & S_IFDIR != 0 {
                    return Err(EISDIR);
                } else {
                    if old_file.mode.load(Ordering::Relaxed) & S_IFDIR != 0 {
                        return Err(ENOTDIR);
                    }
                }

                entries.remove(new_idx);

                // SAFETY: `parent_lock` has done the synchronization
                self.size.fetch_sub(1, Ordering::Relaxed);

                // The last reference to the inode is held by some dentry
                // and will be released when the dentry is released

                // SAFETY: `new_file_lock` has done the synchronization
                new_file.nlink.fetch_sub(1, Ordering::Relaxed);
                *new_file.ctime.lock() = now;
            }

            rename_old(&mut entries[old_entry_idx], &old_file, new_dentry, now);
            *self.mtime.lock() = now;
        } else {
            // Cross-directory rename - handle similar to same directory case

            // Get new parent directory
            let new_parent_inode = new_dentry.parent().get_inode()?;
            assert!(new_parent_inode.is_dir());
            let new_parent = (new_parent_inode.as_ref() as &dyn Any)
                .downcast_ref::<DirectoryInode>()
                .expect("new parent must be a DirectoryInode");

            let old_parent_lock = Task::block_on(self.rwsem.write());
            let new_parent_lock = Task::block_on(new_parent_inode.rwsem.write());

            let old_ino = old_file.ino;
            let new_ino = new_file.as_ref().ok().map(|f| f.ino);
            let old_name = old_dentry.get_name();
            let new_name = new_dentry.get_name();

            // Find the old entry in the old directory
            let old_entries = self.entries.access_mut(old_parent_lock.prove_mut());
            let old_pos = old_entries
                .iter()
                .position(|(name, ino)| *ino == old_ino && *name == old_name)
                .ok_or(ENOENT)?;

            // Find the new entry in the new directory (if it exists)
            let new_entries = new_parent.entries.access_mut(new_parent_lock.prove_mut());
            let has_new = new_entries
                .iter()
                .position(|(name, ino)| Some(*ino) == new_ino && *name == new_name)
                .is_some();

            let now = Instant::now();

            if has_new {
                // Replace existing file (i.e. move the old and unlink the new)
                let new_file = new_file.unwrap();
                let new_file_lock = Task::block_on(new_file.rwsem.write());

                if old_file.mode.load(Ordering::Relaxed) & S_IFDIR != 0
                    && new_file.mode.load(Ordering::Relaxed) & S_IFDIR == 0
                {
                    return Err(ENOTDIR);
                }

                // Unlink the old file that was replaced
                new_parent.do_unlink(
                    &new_file,
                    &new_name,
                    new_entries,
                    now,
                    false,
                    new_parent_lock.prove_mut(),
                    new_file_lock.prove_mut(),
                )?;
            } else {
                new_parent.size.fetch_add(1, Ordering::Relaxed);
            }

            // Remove from old directory
            old_entries.remove(old_pos);

            // Add new entry
            new_entries.push((new_name, old_ino));

            self.size.fetch_sub(1, Ordering::Relaxed);
            *self.mtime.lock() = now;
            *old_file.ctime.lock() = now;
        }

        Task::block_on(dcache::d_exchange(old_dentry, new_dentry));

        Ok(())
    }
}

define_struct_inode! {
    struct SymlinkInode {
        target: Arc<[u8]>,
    }
}

impl SymlinkInode {
    fn new(ino: Ino, vfs: Weak<dyn Vfs>, target: Arc<[u8]>) -> Arc<Self> {
        Self::new_locked(ino, vfs, |inode, _| unsafe {
            let len = target.len();
            addr_of_mut_field!(inode, target).write(target);

            addr_of_mut_field!(&mut *inode, mode).write((S_IFLNK | 0o777).into());
            addr_of_mut_field!(&mut *inode, size).write((len as u64).into());
            addr_of_mut_field!(&mut *inode, ctime).write(Spin::new(Instant::now()));
            addr_of_mut_field!(&mut *inode, mtime).write(Spin::new(Instant::now()));
            addr_of_mut_field!(&mut *inode, atime).write(Spin::new(Instant::now()));
        })
    }
}

impl Inode for SymlinkInode {
    fn readlink(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        buffer
            .fill(self.target.as_ref())
            .map(|result| result.allow_partial())
    }

    fn chmod(&self, _: Mode) -> KResult<()> {
        Ok(())
    }
}

define_struct_inode! {
    pub struct FileInode {
        pages: PageCache,
    }
}

impl Debug for FileInode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "FileInode({:?})", self.idata)
    }
}

impl FileInode {
    pub fn new(ino: Ino, vfs: Weak<dyn Vfs>, size: usize, mode: Mode) -> Arc<Self> {
        let inode = Arc::new_cyclic(|weak_self: &Weak<FileInode>| FileInode {
            idata: InodeData::new(ino, vfs),
            pages: PageCache::new(weak_self.clone()),
        });

        inode
            .mode
            .store(S_IFREG | (mode & 0o777), Ordering::Relaxed);
        inode.nlink.store(1, Ordering::Relaxed);
        inode.size.store(size as u64, Ordering::Relaxed);
        inode
    }
}

impl PageCacheBackend for FileInode {
    fn read_page(&self, _cache_page: &mut CachePage, _offset: usize) -> KResult<usize> {
        Ok(PAGE_SIZE)
    }

    fn write_page(&self, _page: &CachePage, _offset: usize) -> KResult<usize> {
        Ok(PAGE_SIZE)
    }

    fn size(&self) -> usize {
        self.size.load(Ordering::Relaxed) as usize
    }
}

impl Inode for FileInode {
    fn page_cache(&self) -> Option<&PageCache> {
        Some(&self.pages)
    }

    fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        let lock = Task::block_on(self.rwsem.write());
        Task::block_on(self.pages.read(buffer, offset))
    }

    fn write(&self, stream: &mut dyn Stream, offset: WriteOffset) -> KResult<usize> {
        // TODO: We don't need that strong guarantee, find some way to avoid locks
        let lock = Task::block_on(self.rwsem.write());

        let mut store_new_end = None;
        let offset = match offset {
            WriteOffset::Position(offset) => offset,
            WriteOffset::End(end) => {
                store_new_end = Some(end);

                // SAFETY: `lock` has done the synchronization
                self.size.load(Ordering::Relaxed) as usize
            }
        };

        let wrote = Task::block_on(self.pages.write(stream, offset))?;
        let cursor_end = offset + wrote;

        if let Some(store_end) = store_new_end {
            *store_end = cursor_end;
        }

        // SAFETY: `lock` has done the synchronization
        *self.mtime.lock() = Instant::now();
        self.size.store(cursor_end as u64, Ordering::Relaxed);

        Ok(wrote)
    }

    fn truncate(&self, length: usize) -> KResult<()> {
        let lock = Task::block_on(self.rwsem.write());
        Task::block_on(self.pages.resize(length))?;
        self.size.store(length as u64, Ordering::Relaxed);
        *self.mtime.lock() = Instant::now();
        Ok(())
    }

    fn chmod(&self, mode: Mode) -> KResult<()> {
        let _vfs = acquire(&self.vfs)?;
        let _lock = Task::block_on(self.rwsem.write());

        // SAFETY: `rwsem` has done the synchronization
        let old = self.mode.load(Ordering::Relaxed);
        self.mode
            .store((old & !0o777) | (mode & 0o777), Ordering::Relaxed);
        *self.ctime.lock() = Instant::now();

        Ok(())
    }
}

impl_any!(TmpFs);
pub(super) struct TmpFs {
    next_ino: AtomicIno,
    readonly: bool,
    rename_lock: Mutex<()>,
}

impl Vfs for TmpFs {
    fn io_blksize(&self) -> usize {
        4096
    }

    fn fs_devid(&self) -> DevId {
        2
    }

    fn is_read_only(&self) -> bool {
        self.readonly
    }
}

impl TmpFs {
    pub(super) fn assign_ino(&self) -> Ino {
        self.next_ino.fetch_add(1, Ordering::AcqRel)
    }

    pub fn create(readonly: bool) -> KResult<(Arc<TmpFs>, Arc<DirectoryInode>)> {
        let tmpfs = Arc::new(Self {
            next_ino: AtomicIno::new(1),
            readonly,
            rename_lock: Mutex::new(()),
        });

        let weak = Arc::downgrade(&tmpfs);
        let root_dir = DirectoryInode::new(0, weak, 0o755);

        Ok((tmpfs, root_dir))
    }
}

struct TmpFsMountCreator;

impl MountCreator for TmpFsMountCreator {
    fn create_mount(&self, _source: &str, flags: u64, mp: &Arc<Dentry>) -> KResult<Mount> {
        let (fs, root_inode) = TmpFs::create(flags & MS_RDONLY != 0)?;

        Mount::new(mp, fs, root_inode)
    }

    fn check_signature(&self, _: &[u8]) -> KResult<bool> {
        Ok(true)
    }
}

pub fn init() {
    register_filesystem("tmpfs", Arc::new(TmpFsMountCreator)).unwrap();
}
