use crate::io::Stream;
use crate::kernel::constants::{EINVAL, EIO, EISDIR};
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
use core::{ops::ControlFlow, sync::atomic::Ordering};
use eonix_runtime::task::Task;
use eonix_sync::{AsProof as _, AsProofMut as _, Locked, ProofMut};
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
        })
    }
}

impl Inode for NodeInode {
    fn devid(&self) -> KResult<DevId> {
        Ok(self.devid)
    }
}

define_struct_inode! {
    struct DirectoryInode {
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
        })
    }

    fn link(&self, name: Arc<[u8]>, file: &dyn Inode, dlock: ProofMut<'_, ()>) {
        // SAFETY: Only `unlink` will do something based on `nlink` count
        //         No need to synchronize here
        file.nlink.fetch_add(1, Ordering::Relaxed);

        // SAFETY: `rwsem` has done the synchronization
        self.size.fetch_add(1, Ordering::Relaxed);

        self.entries.access_mut(dlock).push((name, file.ino));
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
        let file = FileInode::new(ino, self.vfs.clone(), mode);

        self.link(at.name().clone(), file.as_ref(), rwsem.prove_mut());
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

        self.link(at.name().clone(), file.as_ref(), rwsem.prove_mut());
        at.save_reg(file)
    }

    fn symlink(&self, at: &Arc<Dentry>, target: &[u8]) -> KResult<()> {
        let vfs = acquire(&self.vfs)?;
        let vfs = astmp(&vfs);

        let rwsem = Task::block_on(self.rwsem.write());

        let ino = vfs.assign_ino();
        let file = SymlinkInode::new(ino, self.vfs.clone(), target.into());

        self.link(at.name().clone(), file.as_ref(), rwsem.prove_mut());
        at.save_symlink(file)
    }

    fn mkdir(&self, at: &Dentry, mode: Mode) -> KResult<()> {
        let vfs = acquire(&self.vfs)?;
        let vfs = astmp(&vfs);

        let rwsem = Task::block_on(self.rwsem.write());

        let ino = vfs.assign_ino();
        let newdir = DirectoryInode::new(ino, self.vfs.clone(), mode);

        self.link(at.name().clone(), newdir.as_ref(), rwsem.prove_mut());
        at.save_dir(newdir)
    }

    fn unlink(&self, at: &Arc<Dentry>) -> KResult<()> {
        let _vfs = acquire(&self.vfs)?;

        let dlock = Task::block_on(self.rwsem.write());

        let file = at.get_inode()?;
        let _flock = file.rwsem.write();

        // SAFETY: `flock` has done the synchronization
        if file.mode.load(Ordering::Relaxed) & S_IFDIR != 0 {
            return Err(EISDIR);
        }

        let entries = self.entries.access_mut(dlock.prove_mut());
        entries.retain(|(_, ino)| *ino != file.ino);

        assert_eq!(
            entries.len() as u64,
            // SAFETY: `dlock` has done the synchronization
            self.size.fetch_sub(1, Ordering::Relaxed) - 1
        );

        // SAFETY: `flock` has done the synchronization
        let file_nlink = file.nlink.fetch_sub(1, Ordering::Relaxed) - 1;

        if file_nlink == 0 {
            // Remove the file inode from the inode cache
            // The last reference to the inode is held by some dentry
            // and will be released when the dentry is released
            //
            // TODO: Should we use some inode cache in tmpfs?
            //
            // vfs.icache.lock().retain(|ino, _| *ino != file.ino);
        }

        // Postpone the invalidation of the dentry and inode until the
        // last reference to the dentry is released
        //
        // But we can remove it from the dentry cache immediately
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
    struct FileInode {
        filedata: Locked<Vec<u8>, ()>,
    }
}

impl FileInode {
    fn new(ino: Ino, vfs: Weak<dyn Vfs>, mode: Mode) -> Arc<Self> {
        Self::new_locked(ino, vfs, |inode, rwsem| unsafe {
            addr_of_mut_field!(inode, filedata).write(Locked::new(vec![], rwsem));

            addr_of_mut_field!(&mut *inode, mode).write((S_IFREG | (mode & 0o777)).into());
            addr_of_mut_field!(&mut *inode, nlink).write(1.into());
        })
    }
}

impl Inode for FileInode {
    fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        // TODO: We don't need that strong guarantee, find some way to avoid locks
        let lock = Task::block_on(self.rwsem.read());

        match self.filedata.access(lock.prove()).split_at_checked(offset) {
            Some((_, data)) => buffer.fill(data).map(|result| result.allow_partial()),
            None => Ok(0),
        }
    }

    fn write(&self, stream: &mut dyn Stream, offset: WriteOffset) -> KResult<usize> {
        // TODO: We don't need that strong guarantee, find some way to avoid locks
        let lock = Task::block_on(self.rwsem.write());
        let filedata = self.filedata.access_mut(lock.prove_mut());

        let mut store_new_end = None;
        let offset = match offset {
            WriteOffset::Position(offset) => offset,
            WriteOffset::End(end) => {
                store_new_end = Some(end);

                // SAFETY: `lock` has done the synchronization
                self.size.load(Ordering::Relaxed) as usize
            }
        };

        let mut pos = offset;
        loop {
            if pos >= filedata.len() {
                filedata.resize(pos + 4096, 0);
            }

            match stream.poll_data(&mut filedata[pos..])? {
                Some(data) => pos += data.len(),
                None => break,
            }
        }

        filedata.resize(pos, 0);
        if let Some(store_end) = store_new_end {
            *store_end = pos;
        }

        // SAFETY: `lock` has done the synchronization
        self.size.store(pos as u64, Ordering::Relaxed);
        Ok(pos - offset)
    }

    fn truncate(&self, length: usize) -> KResult<()> {
        // TODO: We don't need that strong guarantee, find some way to avoid locks
        let lock = Task::block_on(self.rwsem.write());
        let filedata = self.filedata.access_mut(lock.prove_mut());

        // SAFETY: `lock` has done the synchronization
        self.size.store(length as u64, Ordering::Relaxed);
        filedata.resize(length, 0);

        Ok(())
    }

    fn chmod(&self, mode: Mode) -> KResult<()> {
        let _vfs = acquire(&self.vfs)?;
        let _lock = Task::block_on(self.rwsem.write());

        // SAFETY: `rwsem` has done the synchronization
        let old = self.mode.load(Ordering::Relaxed);
        self.mode
            .store((old & !0o777) | (mode & 0o777), Ordering::Relaxed);
        Ok(())
    }
}

impl_any!(TmpFs);
struct TmpFs {
    next_ino: AtomicIno,
    readonly: bool,
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
    fn assign_ino(&self) -> Ino {
        self.next_ino.fetch_add(1, Ordering::AcqRel)
    }

    pub fn create(readonly: bool) -> KResult<(Arc<dyn Vfs>, Arc<dyn Inode>)> {
        let tmpfs = Arc::new(Self {
            next_ino: AtomicIno::new(1),
            readonly,
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
}

pub fn init() {
    register_filesystem("tmpfs", Arc::new(TmpFsMountCreator)).unwrap();
}
