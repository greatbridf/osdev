use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::{
    io::{Buffer, ByteBuffer, Stream},
    kernel::{
        block::BlockDevice,
        constants::{EEXIST, EINVAL, EIO, ENOSYS, S_IFDIR, S_IFREG},
        timer::Instant,
        vfs::{
            dentry::{dcache, Dentry},
            inode::{
                define_struct_inode, AtomicNlink, Ino, Inode, InodeData, Mode, RenameData,
                WriteOffset,
            },
            mount::{register_filesystem, Mount, MountCreator},
            s_isdir, s_isreg,
            vfs::Vfs,
            DevId, FsContext,
        },
    },
    path::Path,
    prelude::*,
};
use alloc::{
    collections::btree_map::{BTreeMap, Entry},
    sync::{Arc, Weak},
};
use another_ext4::{
    Block, BlockDevice as Ext4BlockDeviceTrait, Ext4, FileType, InodeMode, PBlockId,
};
use eonix_runtime::task::Task;
use eonix_sync::RwLock;

pub struct Ext4BlockDevice {
    device: Arc<BlockDevice>,
}

impl Ext4BlockDevice {
    pub fn new(device: Arc<BlockDevice>) -> Self {
        Self { device }
    }
}

impl Ext4BlockDeviceTrait for Ext4BlockDevice {
    fn read_block(&self, block_id: PBlockId) -> Block {
        let mut buffer = [0u8; 4096];
        let mut byte_buffer = ByteBuffer::new(buffer.as_mut_slice());

        let _ = self
            .device
            .read_some((block_id as usize) * 4096, &mut byte_buffer)
            .expect("Failed to read from block device");

        Block {
            id: block_id,
            data: buffer,
        }
    }

    fn write_block(&self, block: &another_ext4::Block) {
        let _ = self
            .device
            .write_some((block.id as usize) * 4096, &block.data);
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

    fn modify_inode_stat(&self, ino: u32, size: Option<u64>, mtime: u32) {
        let _ = self
            .inner
            .setattr(ino, None, None, None, size, None, Some(mtime), None, None);
    }

    fn create_inode_stat(&self, parent: u32, child: u32, mtime: u32) {
        let _ = self.inner.setattr(
            parent,
            None,
            None,
            None,
            None,
            None,
            Some(mtime),
            None,
            None,
        );
        let _ = self
            .inner
            .setattr(child, None, None, None, None, None, Some(mtime), None, None);
    }

    fn chmod_stat(&self, ino: u32, new_mode: u16, ctime: u32) {
        let _ = self.inner.setattr(
            ino,
            Some(InodeMode::from_bits_retain(new_mode.try_into().unwrap())),
            None,
            None,
            None,
            None,
            None,
            Some(ctime),
            None,
        );
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
                        .insert(Ext4Inode::File(Arc::new(FileInode { idata })))
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
                        .insert(Ext4Inode::File(Arc::new(FileInode { idata })))
                        .clone()
                        .into_inner()
                }
            }
        }
    }
}

impl Ext4Fs {
    pub fn create(device: Arc<BlockDevice>) -> KResult<(Arc<Self>, Arc<dyn Inode>)> {
        let ext4_device = Ext4BlockDevice::new(device.clone());
        let ext4 = Ext4::load(Arc::new(ext4_device)).unwrap();

        let ext4fs = Arc::new(Self {
            inner: ext4,
            device,
            icache: RwLock::new(BTreeMap::new()),
        });

        let root_inode = {
            let mut icache = Task::block_on(ext4fs.icache.write());
            let root_inode = ext4fs.inner.read_root_inode();

            ext4fs.get_or_insert(
                &mut icache,
                InodeData {
                    ino: root_inode.id as Ino,
                    size: AtomicU64::new(root_inode.inode.size()),
                    nlink: AtomicNlink::new(root_inode.inode.link_count() as u64),
                    uid: AtomicU32::new(root_inode.inode.uid()),
                    gid: AtomicU32::new(root_inode.inode.gid()),
                    mode: AtomicU32::new(root_inode.inode.mode().bits() as u32),
                    atime: Spin::new(Instant::new(
                        root_inode.inode.atime() as _,
                        root_inode.inode.atime_extra() as _,
                    )),
                    ctime: Spin::new(Instant::new(
                        root_inode.inode.ctime() as _,
                        root_inode.inode.ctime_extra() as _,
                    )),
                    mtime: Spin::new(Instant::new(
                        root_inode.inode.mtime() as _,
                        root_inode.inode.mtime_extra() as _,
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
}

impl Ext4Inode {
    fn into_inner(self) -> Arc<dyn Inode> {
        match self {
            Ext4Inode::File(inode) => inode,
            Ext4Inode::Dir(inode) => inode,
        }
    }
}

define_struct_inode! {
    struct FileInode;
}

define_struct_inode! {
    struct DirInode;
}

impl FileInode {
    pub fn new(ino: Ino, vfs: Weak<dyn Vfs>, mode: Mode) -> Arc<Self> {
        Arc::new_cyclic(|_| FileInode {
            idata: {
                let inode_data = InodeData::new(ino, vfs);
                inode_data
                    .mode
                    .store(S_IFREG | (mode & 0o777), Ordering::Relaxed);
                inode_data.nlink.store(1, Ordering::Relaxed);
                inode_data
            },
        })
    }
}

impl Inode for FileInode {
    fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let mut temp_buf = vec![0u8; buffer.total()];
        match ext4fs.inner.read(self.ino as u32, offset, &mut temp_buf) {
            Ok(bytes_read) => {
                let _ = buffer.fill(&temp_buf[..bytes_read])?;
                Ok(buffer.wrote())
            }
            Err(e) => Err(e.code() as u32),
        }
    }

    fn write(&self, stream: &mut dyn Stream, offset: WriteOffset) -> KResult<usize> {
        let _lock = Task::block_on(self.rwsem.write());

        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let mut temp_buf = vec![0u8; 4096];
        let mut total_written = 0;

        let mut store_new_end = None;
        let offset = match offset {
            WriteOffset::Position(offset) => offset,
            // TODO: here need to add some operate
            WriteOffset::End(end) => {
                store_new_end = Some(end);
                self.size.load(Ordering::Relaxed) as usize
            }
        };

        while let Some(data) = stream.poll_data(&mut temp_buf)? {
            let written = ext4fs
                .inner
                .write(self.ino as u32, offset + total_written, data)
                .unwrap();
            total_written += written;
            if written < data.len() {
                break;
            }
        }

        if let Some(store_end) = store_new_end {
            *store_end = offset + total_written;
        }
        let mtime = Instant::now();
        *self.mtime.lock() = mtime;
        let new_size = (offset + total_written) as u64;
        self.size
            .store(offset as u64 + total_written as u64, Ordering::Relaxed);
        ext4fs.modify_inode_stat(
            self.ino as u32,
            Some(new_size),
            mtime.since_epoch().as_secs() as u32,
        );

        Ok(total_written)
    }

    fn chmod(&self, mode: Mode) -> KResult<()> {
        let _lock = Task::block_on(self.rwsem.write());

        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();
        let old_mode = self.mode.load(Ordering::Relaxed);
        let new_mode = (old_mode & !0o777) | (mode & 0o777);

        let now = Instant::now();
        ext4fs.chmod_stat(
            self.ino as u32,
            new_mode as u16,
            now.since_epoch().as_secs() as u32,
        );

        // SAFETY: `rwsem` has done the synchronization
        self.mode.store(new_mode, Ordering::Relaxed);
        *self.ctime.lock() = now;

        Ok(())
    }

    // TODO
    fn truncate(&self, length: usize) -> KResult<()> {
        Ok(())
    }
}

impl DirInode {
    fn new(ino: Ino, vfs: Weak<dyn Vfs>, mode: Mode) -> Arc<Self> {
        Arc::new_cyclic(|_| DirInode {
            idata: {
                let inode_data = InodeData::new(ino, vfs);
                inode_data
                    .mode
                    .store(S_IFDIR | (mode & 0o777), Ordering::Relaxed);
                inode_data.nlink.store(2, Ordering::Relaxed);
                inode_data.size.store(4096, Ordering::Relaxed);
                inode_data
            },
        })
    }

    fn update_time(&self, time: Instant) {
        *self.ctime.lock() = time;
        *self.mtime.lock() = time;
    }

    fn update_child_time(&self, child: &dyn Inode, time: Instant) {
        self.update_time(time);
        *child.ctime.lock() = time;
        *child.mtime.lock() = time;
    }

    fn link_file(&self) {
        self.size.fetch_add(1, Ordering::Relaxed);
    }

    fn link_dir(&self) {
        self.nlink.fetch_add(1, Ordering::Relaxed);
        self.size.fetch_add(1, Ordering::Relaxed);
    }

    fn unlink_dir(&self) {
        self.nlink.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Inode for DirInode {
    fn lookup(&self, dentry: &Arc<Dentry>) -> KResult<Option<Arc<dyn Inode>>> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let name = dentry.get_name();
        let name = String::from_utf8_lossy(&name);
        let lookup_result = ext4fs.inner.lookup(self.ino as u32, &name);

        // TODO: wtf
        //const EXT4_ERROR_ENOENT: Ext4Error_ = Ext4Error_::new(ErrCode::ENOENT);
        let attr = match lookup_result {
            Ok(inode_id) => ext4fs.inner.getattr(inode_id).unwrap(),
            //Err(EXT4_ERROR_ENOENT) => return Ok(None),
            Err(error) => return Err(error.code() as u32),
        };

        // Fast path: if the inode is already in the cache, return it.
        if let Some(inode) = ext4fs.try_get(&Task::block_on(ext4fs.icache.read()), attr.ino as u64)
        {
            return Ok(Some(inode));
        }

        let file_type_bits = match attr.ftype {
            FileType::RegularFile => InodeMode::FILE.bits(),
            FileType::Directory => InodeMode::DIRECTORY.bits(),
            FileType::CharacterDev => InodeMode::CHARDEV.bits(),
            FileType::BlockDev => InodeMode::BLOCKDEV.bits(),
            FileType::Fifo => InodeMode::FIFO.bits(),
            FileType::Socket => InodeMode::SOCKET.bits(),
            FileType::SymLink => InodeMode::SOFTLINK.bits(),
            FileType::Unknown => 0,
        };

        let perm_bits = attr.perm.bits() & InodeMode::PERM_MASK.bits();
        let mode = file_type_bits | perm_bits;

        // Create a new inode based on the attributes.
        let mut icache = Task::block_on(ext4fs.icache.write());
        let inode = ext4fs.get_or_insert(
            &mut icache,
            InodeData {
                ino: attr.ino as Ino,
                size: AtomicU64::new(attr.size),
                nlink: AtomicNlink::new(attr.links as _),
                uid: AtomicU32::new(attr.uid),
                gid: AtomicU32::new(attr.gid),
                mode: AtomicU32::new(mode as u32),
                atime: Spin::new(Instant::new(attr.atime as _, 0)),
                ctime: Spin::new(Instant::new(attr.ctime as _, 0)),
                mtime: Spin::new(Instant::new(attr.mtime as _, 0)),
                rwsem: RwLock::new(()),
                vfs: self.vfs.clone(),
            },
        );

        Ok(Some(inode))
    }

    fn do_readdir(
        &self,
        offset: usize,
        callback: &mut dyn FnMut(&[u8], Ino) -> KResult<core::ops::ControlFlow<(), ()>>,
    ) -> KResult<usize> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let entries = ext4fs
            .inner
            .listdir(self.ino as u32)
            .map_err(|err| err.code() as u32)?;

        let entries_to_process = if offset < entries.len() {
            &entries[offset..]
        } else {
            &entries[0..0]
        };
        let mut current_offset = 0;
        for entry in entries_to_process {
            let name_string = entry.name();
            let name = name_string.as_bytes();
            let inode = entry.inode() as Ino;

            if callback(name, inode)?.is_break() {
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

        let new_ino = ext4fs
            .inner
            .create(
                self.ino as u32,
                &name,
                InodeMode::from_bits_retain((mode | S_IFREG) as u16),
            )
            .unwrap();

        let file = FileInode::new(new_ino as u64, self.vfs.clone(), mode);
        let now = Instant::now();
        self.update_child_time(file.as_ref(), now);
        self.link_file();

        ext4fs.create_inode_stat(self.ino as u32, new_ino, now.since_epoch().as_secs() as u32);

        at.save_reg(file)
    }

    fn mkdir(&self, at: &Dentry, mode: Mode) -> KResult<()> {
        let _lock = Task::block_on(self.rwsem.write());

        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let name = at.get_name();
        let name = String::from_utf8_lossy(&name);

        let new_ino = ext4fs
            .inner
            .mkdir(
                self.ino as u32,
                &name,
                InodeMode::from_bits_retain((mode | S_IFDIR) as u16),
            )
            .unwrap();

        let new_dir = DirInode::new(new_ino as u64, self.vfs.clone(), mode);
        let now = Instant::now();
        self.update_child_time(new_dir.as_ref(), now);
        self.link_dir();

        ext4fs.create_inode_stat(self.ino as u32, new_ino, now.since_epoch().as_secs() as u32);

        at.save_dir(new_dir)
    }

    fn unlink(&self, at: &Arc<Dentry>) -> KResult<()> {
        let _dir_lock = Task::block_on(self.rwsem.write());

        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let file = at.get_inode()?;

        let name = at.get_name();
        let name = String::from_utf8_lossy(&name);
        let _file_lock = Task::block_on(file.rwsem.write());

        if file.is_dir() {
            let _ = ext4fs.inner.rmdir(self.ino as u32, &name);
            self.unlink_dir();
        } else {
            let _ = ext4fs.inner.unlink(self.ino as u32, &name);
        }
        let now = Instant::now();
        self.update_time(now);
        ext4fs.modify_inode_stat(self.ino as u32, None, now.since_epoch().as_secs() as u32);

        dcache::d_remove(at);

        Ok(())
    }

    fn chmod(&self, mode: Mode) -> KResult<()> {
        let _lock = Task::block_on(self.rwsem.write());

        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();
        let old_mode = self.mode.load(Ordering::Relaxed);
        let new_mode = (old_mode & !0o777) | (mode & 0o777);

        let now = Instant::now();
        ext4fs.chmod_stat(
            self.ino as u32,
            new_mode as u16,
            now.since_epoch().as_secs() as u32,
        );

        // SAFETY: `rwsem` has done the synchronization
        self.mode.store(new_mode, Ordering::Relaxed);
        *self.ctime.lock() = now;

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
            println_warn!("Ext4Fs does not support exchange rename for now");
            return Err(ENOSYS);
        }

        // TODO: may need another lock
        let _lock = Task::block_on(self.rwsem.write());
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let old_file = old_dentry.get_inode()?;
        let new_file = new_dentry.get_inode();
        if no_replace && new_file.is_ok() {
            return Err(EEXIST);
        }

        let name = old_dentry.name();
        let name = core::str::from_utf8(&*name).map_err(|_| EINVAL)?;
        let new_name = new_dentry.name();
        let new_name = core::str::from_utf8(&*new_name).map_err(|_| EINVAL)?;

        ext4fs
            .inner
            .rename(self.ino as u32, name, new_parent.ino as u32, new_name)
            .map_err(|err| err.code() as u32)?;

        // TODO: may need more operations
        let now = Instant::now();
        *old_file.ctime.lock() = now;
        *self.mtime.lock() = now;

        let same_parent = Arc::as_ptr(&new_parent) == &raw const *self;
        if !same_parent {
            *new_parent.mtime.lock() = now;
            if old_file.is_dir() {
                self.nlink.fetch_sub(1, Ordering::Relaxed);
                new_parent.nlink.fetch_add(1, Ordering::Relaxed);
            }
        }

        if let Ok(replaced_file) = new_dentry.get_inode() {
            if !no_replace {
                *replaced_file.ctime.lock() = now;
                replaced_file.nlink.fetch_sub(1, Ordering::Relaxed);
            }
        }

        Task::block_on(dcache::d_exchange(old_dentry, new_dentry));

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
