/// TODO:
/// unlink and rmdir will panic
/// write_direct
/// add page cache
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::io::Stream;
use crate::kernel::constants::{EISDIR, S_IFBLK, S_IFCHR, S_IFDIR, S_IFREG};
use crate::kernel::mem::{PageCache, PageCacheBackend};
use crate::kernel::vfs::dentry::dcache;
use crate::kernel::vfs::inode::{Mode, WriteOffset};
use crate::{
    io::{Buffer, ByteBuffer},
    kernel::{
        block::BlockDevice,
        constants::EIO,
        timer::Instant,
        vfs::{
            dentry::Dentry,
            inode::{define_struct_inode, AtomicNlink, Ino, Inode, InodeData},
            mount::{register_filesystem, Mount, MountCreator},
            s_isdir, s_isreg,
            vfs::Vfs,
            DevId, FsContext,
        },
    },
    path::Path,
    prelude::*,
};
use alloc::sync::Weak;
use alloc::{
    collections::btree_map::{BTreeMap, Entry},
    sync::Arc,
};
use eonix_log::{println, println_debug};
use eonix_runtime::task::Task;
use eonix_sync::{AsProofMut, ProofMut, RwLock};
use ext4_rs::{BlockDevice as Ext4BlockDeviceTrait, Ext4Error, InodeFileType};
use ext4_rs::{Errno, Ext4};

pub struct Ext4BlockDevice {
    device: BlockDevice,
}

impl Ext4BlockDevice {
    pub fn new(device: BlockDevice) -> Self {
        Self { device }
    }
}

impl Ext4BlockDeviceTrait for Ext4BlockDevice {
    /// Here's offest is byte's offset
    fn read_offset(&self, offset: usize) -> Vec<u8> {
        let mut buffer = vec![0u8; 4096];
        let mut byte_buffer = ByteBuffer::new(buffer.as_mut_slice());

        let _ = self
            .device
            .read_some(offset, &mut byte_buffer)
            .expect("Failed to read from block device");

        buffer
    }

    /// Here's offest is byte's offset
    fn write_offset(&self, offset: usize, data: &[u8]) {
        // TODO: may need to check write result
        let _ = self.device.write_some(offset, data);
    }
}

impl_any!(Ext4Fs);
struct Ext4Fs {
    inner: Ext4,
    device: Arc<BlockDevice>,
    icache: RwLock<BTreeMap<Ino, Ext4Inode>>,
}

impl Vfs for Ext4Fs {
    fn io_blksize(&self) -> usize {
        4096
    }

    fn fs_devid(&self) -> DevId {
        self.device.devid()
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

impl Ext4Fs {
    fn try_get(&self, icache: &BTreeMap<Ino, Ext4Inode>, ino: u64) -> Option<Arc<dyn Inode>> {
        icache.get(&ino).cloned().map(Ext4Inode::into_inner)
    }

    fn get_or_insert(
        &self,
        icache: &mut BTreeMap<Ino, Ext4Inode>,
        mut idata: InodeData,
    ) -> Arc<dyn Inode> {
        match icache.entry(idata.ino) {
            Entry::Occupied(occupied) => occupied.get().clone().into_inner(),
            Entry::Vacant(vacant) => {
                let mode = *idata.mode.get_mut();
                if s_isreg(mode) {
                    vacant
                        .insert(Ext4Inode::File(FileInode::with_idata(idata)))
                        .clone()
                        .into_inner()
                } else if s_isdir(mode) {
                    vacant
                        .insert(Ext4Inode::Dir(Arc::new(DirInode { idata })))
                        .clone()
                        .into_inner()
                } else {
                    println_warn!("ext4: Unsupported inode type: {mode:#o}");
                    vacant
                        .insert(Ext4Inode::File(FileInode::with_idata(idata)))
                        .clone()
                        .into_inner()
                }
            }
        }
    }
}

impl Ext4Fs {
    pub fn create(device: Arc<BlockDevice>) -> KResult<(Arc<Self>, Arc<dyn Inode>)> {
        let ext4_device = Ext4BlockDevice::new((*device).clone());
        let ext4 = Ext4::open(Arc::new(ext4_device));

        let ext4fs = Arc::new(Self {
            inner: ext4,
            device,
            icache: RwLock::new(BTreeMap::new()),
        });

        let root_inode = {
            let mut icache = Task::block_on(ext4fs.icache.write());
            let root_inode = ext4fs.inner.get_inode_ref(2);

            ext4fs.get_or_insert(
                &mut icache,
                InodeData {
                    ino: root_inode.inode_num as Ino,
                    size: AtomicU64::new(root_inode.inode.size()),
                    nlink: AtomicNlink::new(root_inode.inode.links_count() as _),
                    uid: AtomicU32::new(root_inode.inode.uid() as _),
                    gid: AtomicU32::new(root_inode.inode.gid() as _),
                    mode: AtomicU32::new(root_inode.inode.mode() as _),
                    atime: Spin::new(Instant::new(
                        root_inode.inode.atime() as _,
                        root_inode.inode.i_atime_extra() as _,
                    )),
                    ctime: Spin::new(Instant::new(
                        root_inode.inode.ctime() as _,
                        root_inode.inode.i_ctime_extra() as _,
                    )),
                    mtime: Spin::new(Instant::new(
                        root_inode.inode.mtime() as _,
                        root_inode.inode.i_mtime_extra() as _,
                    )),
                    rwsem: RwLock::new(()),
                    vfs: Arc::downgrade(&ext4fs) as _,
                },
            )
        };

        Ok((ext4fs, root_inode))
    }
}

#[derive(Clone)]
enum Ext4Inode {
    File(Arc<FileInode>),
    Dir(Arc<DirInode>),
    Node(Arc<NodeInode>),
    SymLink(Arc<SymlinkInode>),
}

impl Ext4Inode {
    fn into_inner(self) -> Arc<dyn Inode> {
        match self {
            Ext4Inode::File(inode) => inode,
            Ext4Inode::Dir(inode) => inode,
            Ext4Inode::Node(inode) => inode,
            Ext4Inode::SymLink(inode) => inode,
        }
    }
}

define_struct_inode! {
    struct FileInode {
        page_cache: PageCache,
    }
}

impl FileInode {
    fn with_idata(idata: InodeData) -> Arc<Self> {
        let size = idata.size.load(Ordering::Relaxed) as usize;
        let inode = Arc::new_cyclic(|weak_self: &Weak<FileInode>| Self {
            idata,
            page_cache: PageCache::new(weak_self.clone(), size),
        });

        inode
    }

    pub fn new(ino: Ino, vfs: Weak<dyn Vfs>, mode: Mode) -> Arc<Self> {
        Arc::new_cyclic(|weak_self: &Weak<FileInode>| FileInode {
            idata: {
                let inode_data = InodeData::new(ino, vfs);
                inode_data
                    .mode
                    .store(S_IFREG | (mode & 0o777), Ordering::Relaxed);
                inode_data.nlink.store(1, Ordering::Relaxed);
                inode_data
            },
            page_cache: PageCache::new(weak_self.clone(), 0),
        })
    }
}

impl PageCacheBackend for FileInode {
    fn read_page(&self, page: &mut crate::kernel::mem::CachePage, offset: usize) -> KResult<usize> {
        self.read_direct(page, offset)
    }

    fn write_page(
        &self,
        page: &mut crate::kernel::mem::CachePageStream,
        offset: usize,
    ) -> KResult<usize> {
        self.write_direct(page, offset)
    }
}

impl Inode for FileInode {
    fn page_cache(&self) -> Option<&PageCache> {
        Some(&self.page_cache)
    }

    fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        Task::block_on(self.page_cache.read(buffer, offset))
    }

    fn read_direct(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let mut temp_buf = vec![0u8; buffer.total()];
        match ext4fs.inner.read_at(self.ino as u32, offset, &mut temp_buf) {
            Ok(bytes_read) => {
                let _ = buffer.fill(&temp_buf[..bytes_read])?;
                Ok(buffer.wrote())
            }
            Err(e) => Err(e.error() as u32),
        }
    }

    fn write(&self, stream: &mut dyn Stream, offset: WriteOffset) -> KResult<usize> {
        let _lock = Task::block_on(self.rwsem.write());

        let mut store_new_end = None;
        let offset = match offset {
            WriteOffset::Position(offset) => offset,
            WriteOffset::End(end) => {
                store_new_end = Some(end);
                self.size.load(Ordering::Relaxed) as usize
            }
        };
        let wrote = Task::block_on(self.page_cache.write(stream, offset))?;
        let cursor_end = offset + wrote;

        if let Some(store_end) = store_new_end {
            *store_end = cursor_end;
        }

        // SAFETY: `lock` has done the synchronization
        *self.mtime.lock() = Instant::now();
        self.size.store(cursor_end as u64, Ordering::Relaxed);

        Ok(wrote)
    }

    fn write_direct(&self, stream: &mut dyn Stream, offset: usize) -> KResult<usize> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let mut temp_buf = vec![0u8; 4096];
        let mut total_written = 0;

        while let Some(data) = stream.poll_data(&mut temp_buf)? {
            let written = ext4fs
                .inner
                .write_at(self.ino as u32, offset + total_written, data)
                .unwrap();
            total_written += written;
            if written < data.len() {
                break;
            }
        }

        Ok(total_written)
    }

    fn truncate(&self, length: usize) -> KResult<()> {
        let _lock = Task::block_on(self.rwsem.write());
        Task::block_on(self.page_cache.resize(length))?;

        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();
        let mut inode_ref = ext4fs.inner.get_inode_ref(self.ino as u32);
        let _ = ext4fs.inner.truncate_inode(&mut inode_ref, length as u64);

        self.size.store(length as u64, Ordering::Relaxed);
        *self.mtime.lock() = Instant::now();
        Ok(())
    }

    fn chmod(&self, mode: Mode) -> KResult<()> {
        let _lock = Task::block_on(self.rwsem.write());

        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();
        let old_mode = self.mode.load(Ordering::Relaxed);
        let new_mode = (old_mode & !0o777) | (mode & 0o777);

        let mut inode_ref = ext4fs.inner.get_inode_ref(self.ino as u32);
        // need test
        inode_ref.inode.set_mode(new_mode as u16);
        ext4fs.inner.write_back_inode(&mut inode_ref);

        // SAFETY: `rwsem` has done the synchronization
        self.mode.store(new_mode, Ordering::Relaxed);
        *self.ctime.lock() = Instant::now();

        Ok(())
    }
}

define_struct_inode! {
    // TODO: may need to add a entries
    struct DirInode;
}

impl DirInode {
    fn new(idata: InodeData) -> Arc<Self> {
        let inode = Arc::new(Self { idata });

        inode
    }
}

impl DirInode {
    fn link(&self, file: &dyn Inode) {
        let now = Instant::now();

        // SAFETY: Only `unlink` will do something based on `nlink` count
        //         No need to synchronize here
        file.nlink.fetch_add(1, Ordering::Relaxed);
        *self.ctime.lock() = now;

        // SAFETY: `rwsem` has done the synchronization
        self.size.fetch_add(1, Ordering::Relaxed);
        *self.mtime.lock() = now;
    }

    fn unlink(
        &self,
        file: &Arc<dyn Inode>,
        decrease_size: bool,
        _dir_lock: ProofMut<()>,
        _file_lock: ProofMut<()>,
    ) -> KResult<()> {
        let now = Instant::now();

        // SAFETY: `file_lock` has done the synchronization
        if file.mode.load(Ordering::Relaxed) & S_IFDIR != 0 {
            return Err(EISDIR);
        }

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

impl Inode for DirInode {
    fn lookup(&self, dentry: &Arc<Dentry>) -> KResult<Option<Arc<dyn Inode>>> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let name = dentry.get_name();
        let name = String::from_utf8_lossy(&name);
        let lookup_result = ext4fs.inner.fuse_lookup(self.ino, &name);

        const EXT4_ERROR_ENOENT: Ext4Error = Ext4Error::new(Errno::ENOENT);
        let attr = match lookup_result {
            Ok(attr) => attr,
            Err(EXT4_ERROR_ENOENT) => return Ok(None),
            Err(error) => return Err(error.error() as u32),
        };

        // Fast path: if the inode is already in the cache, return it.
        if let Some(inode) = ext4fs.try_get(&Task::block_on(ext4fs.icache.read()), attr.ino as u64)
        {
            return Ok(Some(inode));
        }

        let extra_perm = attr.perm.bits() as u32 & 0o7000;
        let perm = attr.perm.bits() as u32 & 0o0700;
        let real_perm = extra_perm | perm | perm >> 3 | perm >> 6;

        // Create a new inode based on the attributes.
        let mut icache = Task::block_on(ext4fs.icache.write());
        let inode = ext4fs.get_or_insert(
            &mut icache,
            InodeData {
                ino: attr.ino as Ino,
                size: AtomicU64::new(attr.size),
                nlink: AtomicNlink::new(attr.nlink as _),
                uid: AtomicU32::new(attr.uid),
                gid: AtomicU32::new(attr.gid),
                mode: AtomicU32::new(attr.kind.bits() as u32 | real_perm),
                atime: Spin::new(Instant::new(attr.atime as _, 0)),
                ctime: Spin::new(Instant::new(attr.ctime as _, 0)),
                mtime: Spin::new(Instant::new(attr.mtime as _, 0)),
                rwsem: RwLock::new(()),
                vfs: self.vfs.clone(),
            },
        );

        Ok(Some(inode))
    }

    /// Here's offest is index of the directory item
    fn do_readdir(
        &self,
        offset: usize,
        callback: &mut dyn FnMut(&[u8], Ino) -> KResult<core::ops::ControlFlow<(), ()>>,
    ) -> KResult<usize> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let entries = ext4fs
            .inner
            .fuse_readdir(self.ino as u64, 0, offset as i64)
            .map_err(|err| err.error() as u32)?;
        let mut current_offset = 0;

        for entry in entries {
            let name_len = entry.name_len as usize;
            let name = &entry.name[..name_len];

            if callback(name, entry.inode as Ino)?.is_break() {
                break;
            }

            current_offset += 1;
        }
        Ok(current_offset)
    }

    fn creat(&self, at: &Arc<Dentry>, mode: Mode) -> KResult<()> {
        let _lock = Task::block_on(self.rwsem.write());

        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let name = at.get_name();
        let name = String::from_utf8_lossy(&name);
        let inode_ref = ext4fs
            .inner
            .create(self.ino as u32, &name, (mode | S_IFREG) as u16)
            .unwrap();

        let file = FileInode::new(inode_ref.inode_num as u64, self.vfs.clone(), mode);

        let now = Instant::now();

        *self.ctime.lock() = now;
        // SAFETY: `rwsem` has done the synchronization
        self.size.fetch_add(1, Ordering::Relaxed);
        *self.mtime.lock() = now;

        at.save_reg(file)
    }

    fn mknod(&self, at: &Dentry, mode: Mode, dev: DevId) -> KResult<()> {
        let _lock = Task::block_on(self.rwsem.write());

        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let name = at.get_name();
        let name = String::from_utf8_lossy(&name);
        let inode_ref =
            ext4fs
                .inner
                .fuse_mknod(self.ino, &name, mode & (0o777 | S_IFBLK | S_IFCHR), 0, dev);

        let file = NodeInode::new(
            InodeData::new(inode_ref.unwrap().inode_num.into(), self.vfs.clone()),
            dev,
        );

        self.link(file.as_ref());
        at.save_reg(file)
    }

    /// TODO: the target parameter does not seem to be used.
    fn symlink(&self, at: &Arc<Dentry>, target: &[u8]) -> KResult<()> {
        let _lock = Task::block_on(self.rwsem.write());

        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();
        let mut mode = 0o777;
        let file_type = InodeFileType::S_IFLNK;
        mode |= file_type.bits();

        let name = at.get_name();
        let name = String::from_utf8_lossy(&name);
        let inode_ref = ext4fs.inner.create(self.ino as u32, &name, mode).unwrap();

        let file = SymlinkInode::new(
            InodeData::new(inode_ref.inode_num as u64, self.vfs.clone()),
            target.into(),
        );

        // TODO: may need add a entries
        self.link(file.as_ref());
        at.save_symlink(file)
    }

    fn mkdir(&self, at: &Dentry, mode: Mode) -> KResult<()> {
        let _lock = Task::block_on(self.rwsem.write());

        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let name = at.get_name();
        let name = String::from_utf8_lossy(&name);

        let inode_ref = ext4fs
            .inner
            .create(self.ino as u32, &name, (mode | S_IFDIR) as u16)
            .unwrap();

        let newdir = DirInode::new(InodeData::new(inode_ref.inode_num as u64, self.vfs.clone()));

        self.link(newdir.as_ref());
        at.save_dir(newdir)
    }

    fn unlink(&self, at: &Arc<Dentry>) -> KResult<()> {
        let dir_lock = Task::block_on(self.rwsem.write());

        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let file = at.get_inode()?;

        let name = at.get_name();
        let name = String::from_utf8_lossy(&name);
        let file_lock = Task::block_on(file.rwsem.write());

        if file.is_dir() {
            let _ = ext4fs.inner.fuse_rmdir(self.ino, &name);
        } else {
            let _ = ext4fs.inner.fuse_unlink(self.ino, &name);
        }

        self.unlink(&file, true, dir_lock.prove_mut(), file_lock.prove_mut())?;
        dcache::d_remove(at);

        Ok(())
    }

    fn chmod(&self, mode: Mode) -> KResult<()> {
        let _lock = Task::block_on(self.rwsem.write());

        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();
        let old_mode = self.mode.load(Ordering::Relaxed);
        let new_mode = (old_mode & !0o777) | (mode & 0o777);

        let mut inode_ref = ext4fs.inner.get_inode_ref(self.ino as u32);
        // need test
        inode_ref.inode.set_mode(new_mode as u16);
        ext4fs.inner.write_back_inode(&mut inode_ref);

        // SAFETY: `rwsem` has done the synchronization
        self.mode.store(new_mode, Ordering::Relaxed);
        *self.ctime.lock() = Instant::now();

        Ok(())
    }
}

define_struct_inode! {
    struct NodeInode {
        devid: DevId,
    }
}

impl NodeInode {
    fn new(idata: InodeData, devid: DevId) -> Arc<Self> {
        let inode = Arc::new(Self { idata, devid });

        inode
    }
}

impl Inode for NodeInode {
    fn devid(&self) -> KResult<DevId> {
        Ok(self.devid)
    }
}

define_struct_inode! {
    struct SymlinkInode {
        target: Arc<[u8]>,
    }
}

impl SymlinkInode {
    fn new(idata: InodeData, target: Arc<[u8]>) -> Arc<Self> {
        let inode = Arc::new(Self { idata, target });

        inode
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

struct Ext4MountCreator;

impl MountCreator for Ext4MountCreator {
    fn check_signature(&self, mut first_block: &[u8]) -> KResult<bool> {
        match first_block.split_off(1080..) {
            Some([0x53, 0xef, ..]) => Ok(true), // Superblock signature
            Some(..) => Ok(false),
            None => Err(EIO),
        }
    }

    fn create_mount(&self, source: &str, _flags: u64, mp: &Arc<Dentry>) -> KResult<Mount> {
        let source = source.as_bytes();

        let path = Path::new(source)?;
        let device_dentry =
            Dentry::open_recursive(&FsContext::global(), Dentry::root(), path, true, 0)?;
        let devid = device_dentry.get_inode()?.devid()?;
        let device = BlockDevice::get(devid)?;

        let (ext4fs, root_inode) = Ext4Fs::create(device)?;

        Mount::new(mp, ext4fs, root_inode)
    }
}

pub fn init() {
    register_filesystem("ext4", Arc::new(Ext4MountCreator)).unwrap();
}
