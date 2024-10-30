use alloc::{
    collections::btree_map::BTreeMap,
    sync::{Arc, Weak},
};
use bindings::{EACCES, ENOTDIR, S_IFDIR, S_IFREG};
use core::sync::atomic::Ordering;
use lazy_static::lazy_static;

use crate::{
    io::Buffer,
    kernel::{
        mem::paging::{Page, PageBuffer},
        vfs::{
            dentry::Dentry,
            inode::{define_struct_inode, AtomicIno, Ino, Inode, InodeData},
            mount::{dump_mounts, register_filesystem, Mount, MountCreator},
            vfs::Vfs,
            DevId, ReadDirCallback,
        },
    },
    prelude::*,
    sync::Locked,
};

fn split_len_offset(data: &[u8], len: usize, offset: usize) -> Option<&[u8]> {
    let real_data = data.split_at_checked(len).map(|(data, _)| data)?;

    real_data.split_at_checked(offset).map(|(_, data)| data)
}

pub trait ProcFsFile: Send + Sync {
    fn can_read(&self) -> bool {
        false
    }

    fn can_write(&self) -> bool {
        false
    }

    fn read(&self, _buffer: &mut PageBuffer) -> KResult<usize> {
        Err(EACCES)
    }

    fn write(&self, _buffer: &[u8]) -> KResult<usize> {
        Err(EACCES)
    }
}

pub enum ProcFsNode {
    File(Arc<FileInode>),
    Dir(Arc<DirInode>),
}

impl ProcFsNode {
    fn unwrap(&self) -> Arc<dyn Inode> {
        match self {
            ProcFsNode::File(inode) => inode.clone(),
            ProcFsNode::Dir(inode) => inode.clone(),
        }
    }

    fn ino(&self) -> Ino {
        match self {
            ProcFsNode::File(inode) => inode.ino,
            ProcFsNode::Dir(inode) => inode.ino,
        }
    }
}

define_struct_inode! {
    struct FileInode {
        file: Box<dyn ProcFsFile>,
    }
}

impl FileInode {
    pub fn new(ino: Ino, vfs: Weak<ProcFs>, file: Box<dyn ProcFsFile>) -> Arc<Self> {
        let mut mode = S_IFREG;
        if file.can_read() {
            mode |= 0o444;
        }
        if file.can_write() {
            mode |= 0o200;
        }

        let inode = Self {
            idata: InodeData::new(ino, vfs),
            file,
        };

        inode.idata.mode.store(mode, Ordering::Relaxed);
        inode.idata.nlink.store(1, Ordering::Relaxed);

        Arc::new(inode)
    }
}

impl Inode for FileInode {
    fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        if !self.file.can_read() {
            return Err(EACCES);
        }

        let mut page_buffer = PageBuffer::new(Page::alloc_one());
        let nread = self.file.read(&mut page_buffer)?;

        let data = split_len_offset(page_buffer.as_slice(), nread, offset);

        match data {
            None => Ok(0),
            Some(data) => Ok(buffer.fill(data)?.allow_partial()),
        }
    }
}

define_struct_inode! {
    struct DirInode {
        entries: Locked<Vec<(Arc<[u8]>, ProcFsNode)>, ()>,
    }
}

impl DirInode {
    pub fn new(ino: Ino, vfs: Weak<ProcFs>) -> Arc<Self> {
        Self::new_locked(ino, vfs, |inode, rwsem| unsafe {
            addr_of_mut_field!(inode, entries).write(Locked::new(vec![], rwsem));
            addr_of_mut_field!(inode, mode).write((S_IFDIR | 0o755).into());
            addr_of_mut_field!(inode, nlink).write(1.into());
        })
    }
}

impl Inode for DirInode {
    fn lookup(&self, dentry: &Arc<Dentry>) -> KResult<Option<Arc<dyn Inode>>> {
        let lock = self.rwsem.lock_shared();
        Ok(self
            .entries
            .access(lock.as_ref())
            .iter()
            .find_map(|(name, node)| {
                name.as_ref()
                    .eq(dentry.name().as_ref())
                    .then(|| node.unwrap())
            }))
    }

    fn readdir<'cb, 'r: 'cb>(
        &'r self,
        offset: usize,
        callback: &ReadDirCallback<'cb>,
    ) -> KResult<usize> {
        let lock = self.rwsem.lock_shared();
        Ok(self
            .entries
            .access(lock.as_ref())
            .iter()
            .skip(offset)
            .take_while(|(name, node)| callback(name, node.ino()).is_ok())
            .count())
    }
}

impl_any!(ProcFs);
pub struct ProcFs {
    root_node: Arc<DirInode>,
    next_ino: AtomicIno,
}

impl Vfs for ProcFs {
    fn io_blksize(&self) -> usize {
        4096
    }

    fn fs_devid(&self) -> DevId {
        10
    }

    fn is_read_only(&self) -> bool {
        false
    }
}

lazy_static! {
    static ref ICACHE: Spin<BTreeMap<Ino, ProcFsNode>> = Spin::new(BTreeMap::new());
    static ref GLOBAL_PROCFS: Arc<ProcFs> = {
        let fs: Arc<ProcFs> = Arc::new_cyclic(|weak: &Weak<ProcFs>| ProcFs {
            root_node: DirInode::new(0, weak.clone()),
            next_ino: AtomicIno::new(1),
        });

        fs
    };
}

struct ProcFsMountCreator;

impl ProcFsMountCreator {
    pub fn get() -> Arc<ProcFs> {
        GLOBAL_PROCFS.clone()
    }

    pub fn get_weak() -> Weak<ProcFs> {
        Arc::downgrade(&GLOBAL_PROCFS)
    }
}

impl MountCreator for ProcFsMountCreator {
    fn create_mount(
        &self,
        _source: &str,
        _flags: u64,
        _data: &[u8],
        mp: &Arc<Dentry>,
    ) -> KResult<Mount> {
        let vfs = ProcFsMountCreator::get();
        let root_inode = vfs.root_node.clone();
        Mount::new(mp, vfs, root_inode)
    }
}

pub fn root() -> ProcFsNode {
    let vfs = ProcFsMountCreator::get();
    let root = vfs.root_node.clone();

    ProcFsNode::Dir(root)
}

pub fn creat(
    parent: &ProcFsNode,
    name: &Arc<[u8]>,
    file: Box<dyn ProcFsFile>,
) -> KResult<ProcFsNode> {
    let parent = match parent {
        ProcFsNode::File(_) => return Err(ENOTDIR),
        ProcFsNode::Dir(parent) => parent,
    };

    let fs = ProcFsMountCreator::get();
    let ino = fs.next_ino.fetch_add(1, Ordering::Relaxed);

    let inode = FileInode::new(ino, Arc::downgrade(&fs), file);

    {
        let mut lock = parent.idata.rwsem.lock();
        parent
            .entries
            .access_mut(lock.as_mut())
            .push((name.clone(), ProcFsNode::File(inode.clone())));
    }

    Ok(ProcFsNode::File(inode))
}

pub fn mkdir(parent: &ProcFsNode, name: &[u8]) -> KResult<ProcFsNode> {
    let parent = match parent {
        ProcFsNode::File(_) => return Err(ENOTDIR),
        ProcFsNode::Dir(parent) => parent,
    };

    let fs = ProcFsMountCreator::get();
    let ino = fs.next_ino.fetch_add(1, Ordering::Relaxed);

    let inode = DirInode::new(ino, Arc::downgrade(&fs));

    parent
        .entries
        .access_mut(inode.rwsem.lock().as_mut())
        .push((Arc::from(name), ProcFsNode::Dir(inode.clone())));

    Ok(ProcFsNode::Dir(inode))
}

struct DumpMountsFile;
impl ProcFsFile for DumpMountsFile {
    fn can_read(&self) -> bool {
        true
    }

    fn read(&self, buffer: &mut PageBuffer) -> KResult<usize> {
        dump_mounts(buffer);

        Ok(buffer.len())
    }
}

pub fn init() {
    register_filesystem("procfs", Arc::new(ProcFsMountCreator)).unwrap();

    creat(
        &root(),
        &Arc::from(b"mounts".as_slice()),
        Box::new(DumpMountsFile),
    )
    .unwrap();
}
