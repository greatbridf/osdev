use core::sync::atomic::Ordering;
use alloc::{collections::btree_map::BTreeMap, sync::{Arc, Weak}};
use ext4_rs::{Ext4, InodeFileType};
use ext4_rs::BlockDevice as Ext4BlockDeviceTrait;

use crate::{
    io::{Buffer, ByteBuffer},
    kernel::{
        block::{make_device, BlockDevice},
        constants::{EIO, S_IFDIR, S_IFREG},
        vfs::{
            dentry::Dentry,
            inode::{define_struct_inode, Ino, Inode, InodeData},
            mount::{register_filesystem, Mount, MountCreator},
            vfs::Vfs,
            DevId,
        },
    }, prelude::*
};

pub struct Ext4BlockDevice {
    device: Arc<BlockDevice>,
}

impl Ext4BlockDevice {
    pub fn new(device: Arc<BlockDevice>) -> Self {
        Self {
            device,
        }
    }
}

impl Ext4BlockDeviceTrait for Ext4BlockDevice {
    fn read_offset(&self, offset: usize) -> Vec<u8> {
        let mut buffer = vec![0u8; 4096];
        let mut byte_buffer = ByteBuffer::new(buffer.as_mut_slice());
        match self.device.read_some(offset, &mut byte_buffer) {
            Ok(fill_result) => {
                buffer
            }
            Err(_) => {
                vec![0u8; 4096]
            }
        }
    }

    fn write_offset(&self, offset: usize, data: &[u8]) {
        todo!()
    }
}

impl_any!(Ext4Fs);
struct Ext4Fs {
    inner: Ext4,
    device: Arc<BlockDevice>,
    icache: BTreeMap<Ino, Ext4Inode>,
    weak: Weak<Ext4Fs>,
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
    fn get_or_alloc_inode(&self, ino: Ino, is_directory: bool, size: u32) -> Arc<dyn Inode> {
        self.icache
            .get(&ino)
            .cloned()
            .map(Ext4Inode::unwrap)
            .unwrap_or_else(|| {
                if is_directory {
                    DirInode::new(ino, self.weak.clone(), size)
                } else {
                    FileInode::new(ino, self.weak.clone(), size)
                }
            })
    }
}

impl Ext4Fs {
    pub fn create(device: DevId) -> KResult<(Arc<Self>, Arc<dyn Inode>)>  {
        let device = BlockDevice::get(device)?;
        let ext4_device = Ext4BlockDevice::new(device.clone());
        let ext4 = Ext4::open(Arc::new(ext4_device));
        let mut ext4fs = Arc::new_cyclic(|weak: &Weak<Ext4Fs>| Self {
            inner: ext4,
            device,
            icache: BTreeMap::new(),
            weak: weak.clone(),
        });
        let root_inode_ref = ext4fs.inner.get_inode_ref(2);
        let root_inode = DirInode::new(root_inode_ref.inode_num as Ino, ext4fs.weak.clone(), root_inode_ref.inode.size() as u32);
        Ok((ext4fs, root_inode))
    }
}

#[allow(dead_code)]
#[derive(Clone)]
enum Ext4Inode {
    File(Arc<FileInode>),
    Dir(Arc<DirInode>),
}

impl Ext4Inode {
    fn unwrap(self) -> Arc<dyn Inode> {
        match self {
            Ext4Inode::File(inode) => inode,
            Ext4Inode::Dir(inode) => inode,
        }
    }
}

define_struct_inode! {
    struct FileInode;
}

impl FileInode {
    pub fn new(ino: Ino, weak: Weak<dyn Vfs>, size: u32) -> Arc<Self> {
        let inode = Arc::new(Self {
            idata: InodeData::new(ino, weak),
        });
        inode.nlink.store(1, Ordering::Relaxed);
        inode.mode.store(S_IFREG | 0o644, Ordering::Relaxed);
        inode.size.store(size as u64, Ordering::Relaxed);

        inode
    }
}

impl Inode for FileInode {
    fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();
        
        let mut temp_buf = vec![0u8; buffer.total()];
        match ext4fs.inner.read_at(self.ino.try_into().unwrap(), offset, &mut temp_buf) {
            Ok(bytes_read) => {
                buffer.fill(&temp_buf[..bytes_read]).map_err(|_| EIO)?;
                Ok(bytes_read)
            }
            Err(e) => Err(e.error() as u32),
        }
    }
}

define_struct_inode! {
    struct DirInode;
}

impl DirInode {
    pub fn new(ino: Ino, weak: Weak<dyn Vfs>, size: u32) -> Arc<Self> {
        let inode = Arc::new(Self {
            idata: InodeData::new(ino, weak),
        });
        inode.nlink.store(2, Ordering::Relaxed);
        inode.mode.store(S_IFDIR | 0o644, Ordering::Relaxed);
        inode.size.store(size as u64, Ordering::Relaxed);

        inode
    }
}

impl Inode for DirInode {
    fn lookup(&self, dentry: &Arc<Dentry>) -> KResult<Option<Arc<dyn Inode>>> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let name = str::from_utf8(&**dentry.name()).unwrap();
        match ext4fs.inner.fuse_lookup(self.ino, name) {
            Ok(attr) => {
                let new_ino = attr.ino as Ino;
                let is_dir = attr.kind == InodeFileType::S_IFDIR;
                let size = attr.size;

                let inode = ext4fs.get_or_alloc_inode(new_ino, is_dir, size as u32);
                Ok(Some(inode))
            }
            Err(e) => Err(e.error() as u32),
        }
    }

    fn do_readdir(
            &self,
            offset: usize,
            callback: &mut dyn FnMut(&[u8], Ino) -> KResult<core::ops::ControlFlow<(), ()>>,
        ) -> KResult<usize> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let ext4fs = vfs.as_any().downcast_ref::<Ext4Fs>().unwrap();

        let entries = ext4fs.inner.fuse_readdir(self.ino as u64, 0, offset as i64)
            .map_err(|_| EIO)?;
        let mut current_offset = 0;

        for entry in entries {
            let name_len = entry.name_len as usize;
            let name = &entry.name[..name_len];

            let ino = entry.inode as Ino;
            let inode_ref = ext4fs.inner.get_inode_ref(entry.inode);

            let is_dir = inode_ref.inode.is_dir();
            let size = inode_ref.inode.size();

            ext4fs.get_or_alloc_inode(ino, is_dir, size as u32);

            if callback(name, ino)?.is_break() {
                break;
            }

            current_offset += 1;
        }
        Ok(current_offset)
    }
}

struct Ext4MountCreator;

impl MountCreator for Ext4MountCreator {
    fn create_mount(&self, _source: &str, _flags: u64, mp: &Arc<Dentry>) -> KResult<Mount> {
        // TODO: temporarily the second disk, should generate from _source
        let (ext4fs, root_inode) = 
            Ext4Fs::create(make_device(8, 0))?;

        Mount::new(mp, ext4fs, root_inode)
    }
}

pub fn init() {
    register_filesystem("ext4", Arc::new(Ext4MountCreator)).unwrap();
}
