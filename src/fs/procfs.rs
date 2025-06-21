use crate::kernel::constants::{EACCES, ENOTDIR};
use crate::kernel::timer::Instant;
use crate::{
    io::Buffer,
    kernel::{
        constants::{S_IFDIR, S_IFREG},
        mem::paging::PageBuffer,
        vfs::{
            dentry::Dentry,
            inode::{define_struct_inode, AtomicIno, Ino, Inode, InodeData},
            mount::{dump_mounts, register_filesystem, Mount, MountCreator},
            vfs::Vfs,
            DevId,
        },
    },
    prelude::*,
};
use alloc::sync::{Arc, Weak};
use core::{ops::ControlFlow, sync::atomic::Ordering};
use eonix_runtime::task::Task;
use eonix_sync::{AsProof as _, AsProofMut as _, LazyLock, Locked};
use itertools::Itertools;

#[allow(dead_code)]
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
    pub struct FileInode {
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

        let mut inode = Self {
            idata: InodeData::new(ino, vfs),
            file,
        };

        inode.idata.mode.store(mode, Ordering::Relaxed);
        inode.idata.nlink.store(1, Ordering::Relaxed);
        *inode.ctime.get_mut() = Instant::now();
        *inode.mtime.get_mut() = Instant::now();
        *inode.atime.get_mut() = Instant::now();

        Arc::new(inode)
    }
}

impl Inode for FileInode {
    fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        if !self.file.can_read() {
            return Err(EACCES);
        }

        let mut page_buffer = PageBuffer::new();
        self.file.read(&mut page_buffer)?;

        let data = page_buffer
            .data()
            .split_at_checked(offset)
            .map(|(_, data)| data);

        match data {
            None => Ok(0),
            Some(data) => Ok(buffer.fill(data)?.allow_partial()),
        }
    }
}

define_struct_inode! {
    pub struct DirInode {
        entries: Locked<Vec<(Arc<[u8]>, ProcFsNode)>, ()>,
    }
}

impl DirInode {
    pub fn new(ino: Ino, vfs: Weak<ProcFs>) -> Arc<Self> {
        Self::new_locked(ino, vfs, |inode, rwsem| unsafe {
            addr_of_mut_field!(inode, entries).write(Locked::new(vec![], rwsem));
            addr_of_mut_field!(&mut *inode, mode).write((S_IFDIR | 0o755).into());
            addr_of_mut_field!(&mut *inode, nlink).write(1.into());
            addr_of_mut_field!(&mut *inode, ctime).write(Spin::new(Instant::now()));
            addr_of_mut_field!(&mut *inode, mtime).write(Spin::new(Instant::now()));
            addr_of_mut_field!(&mut *inode, atime).write(Spin::new(Instant::now()));
        })
    }
}

impl Inode for DirInode {
    fn lookup(&self, dentry: &Arc<Dentry>) -> KResult<Option<Arc<dyn Inode>>> {
        let lock = Task::block_on(self.rwsem.read());
        Ok(self
            .entries
            .access(lock.prove())
            .iter()
            .find_map(|(name, node)| {
                name.as_ref()
                    .eq(dentry.name().as_ref())
                    .then(|| node.unwrap())
            }))
    }

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
            .map(|(name, node)| callback(name.as_ref(), node.ino()))
            .take_while(|result| result.map_or(true, |flow| flow.is_continue()))
            .take_while_inclusive(|result| result.is_ok())
            .fold_ok(0, |acc, _| acc + 1)
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

static GLOBAL_PROCFS: LazyLock<Arc<ProcFs>> = LazyLock::new(|| {
    Arc::new_cyclic(|weak: &Weak<ProcFs>| ProcFs {
        root_node: DirInode::new(0, weak.clone()),
        next_ino: AtomicIno::new(1),
    })
});

struct ProcFsMountCreator;

#[allow(dead_code)]
impl ProcFsMountCreator {
    pub fn get() -> Arc<ProcFs> {
        GLOBAL_PROCFS.clone()
    }

    pub fn get_weak() -> Weak<ProcFs> {
        Arc::downgrade(&GLOBAL_PROCFS)
    }
}

impl MountCreator for ProcFsMountCreator {
    fn create_mount(&self, _source: &str, _flags: u64, mp: &Arc<Dentry>) -> KResult<Mount> {
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
    name: Arc<[u8]>,
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
        let lock = Task::block_on(parent.idata.rwsem.write());
        parent
            .entries
            .access_mut(lock.prove_mut())
            .push((name, ProcFsNode::File(inode.clone())));
    }

    Ok(ProcFsNode::File(inode))
}

#[allow(dead_code)]
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
        .access_mut(Task::block_on(inode.rwsem.write()).prove_mut())
        .push((Arc::from(name), ProcFsNode::Dir(inode.clone())));

    Ok(ProcFsNode::Dir(inode))
}

struct DumpMountsFile;
impl ProcFsFile for DumpMountsFile {
    fn can_read(&self) -> bool {
        true
    }

    fn read(&self, buffer: &mut PageBuffer) -> KResult<usize> {
        dump_mounts(&mut buffer.get_writer());

        Ok(buffer.data().len())
    }
}

pub fn init() {
    register_filesystem("procfs", Arc::new(ProcFsMountCreator)).unwrap();

    creat(
        &root(),
        Arc::from(b"mounts".as_slice()),
        Box::new(DumpMountsFile),
    )
    .unwrap();
}

pub struct GenericProcFsFile<ReadFn>
where
    ReadFn: Send + Sync + Fn(&mut PageBuffer) -> KResult<()>,
{
    read_fn: Option<ReadFn>,
}

impl<ReadFn> ProcFsFile for GenericProcFsFile<ReadFn>
where
    ReadFn: Send + Sync + Fn(&mut PageBuffer) -> KResult<()>,
{
    fn can_read(&self) -> bool {
        self.read_fn.is_some()
    }

    fn read(&self, buffer: &mut PageBuffer) -> KResult<usize> {
        self.read_fn.as_ref().ok_or(EACCES)?(buffer).map(|_| buffer.data().len())
    }
}

pub fn populate_root<F>(name: Arc<[u8]>, read_fn: F) -> KResult<()>
where
    F: Send + Sync + Fn(&mut PageBuffer) -> KResult<()> + 'static,
{
    let root = root();

    creat(
        &root,
        name,
        Box::new(GenericProcFsFile {
            read_fn: Some(read_fn),
        }),
    )
    .map(|_| ())
}
