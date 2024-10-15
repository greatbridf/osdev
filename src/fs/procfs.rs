use core::sync::atomic::Ordering;

use alloc::sync::{Arc, Weak};
use bindings::{EACCES, ENOTDIR, S_IFDIR, S_IFREG};

use crate::{
    io::Buffer,
    kernel::{
        mem::paging::{Page, PageBuffer},
        vfs::{
            dentry::Dentry,
            inode::{AtomicIno, Inode, InodeCache, InodeData, InodeOps},
            mount::{dump_mounts, register_filesystem, Mount, MountCreator},
            vfs::Vfs,
            DevId, ReadDirCallback,
        },
    },
    prelude::*,
};

fn split_len_offset(data: &[u8], len: usize, offset: usize) -> Option<&[u8]> {
    let real_data = data.split_at_checked(len).map(|(data, _)| data)?;

    real_data.split_at_checked(offset).map(|(_, data)| data)
}

pub struct ProcFsNode(Arc<Inode>);

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

struct ProcFsFileOps {
    file: Box<dyn ProcFsFile>,
}

impl InodeOps for ProcFsFileOps {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn read(
        &self,
        _: &Inode,
        buffer: &mut dyn Buffer,
        offset: usize,
    ) -> KResult<usize> {
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

struct ProcFsDirectory {
    entries: Mutex<Vec<(Arc<[u8]>, ProcFsNode)>>,
}

impl InodeOps for ProcFsDirectory {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn lookup(
        &self,
        _: &Inode,
        dentry: &Arc<Dentry>,
    ) -> KResult<Option<Arc<Inode>>> {
        Ok(self.entries.lock().iter().find_map(|(name, node)| {
            name.as_ref()
                .eq(dentry.name().as_ref())
                .then(|| node.0.clone())
        }))
    }

    fn readdir<'cb, 'r: 'cb>(
        &self,
        _: &Inode,
        offset: usize,
        callback: &ReadDirCallback<'cb>,
    ) -> KResult<usize> {
        Ok(self
            .entries
            .lock()
            .iter()
            .skip(offset)
            .take_while(|(name, ProcFsNode(inode))| {
                callback(name, inode.ino).is_ok()
            })
            .count())
    }
}

pub struct ProcFs {
    root_node: Arc<Inode>,
    next_ino: AtomicIno,
}

impl Vfs for ProcFs {
    fn io_blksize(&self) -> usize {
        4096
    }

    fn fs_devid(&self) -> DevId {
        10
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

static mut GLOBAL_PROCFS: Option<Arc<ProcFs>> = None;
static mut ICACHE: Option<InodeCache<ProcFs>> = None;

fn get_icache() -> &'static InodeCache<ProcFs> {
    unsafe { ICACHE.as_ref().unwrap() }
}

struct ProcFsMountCreator;

impl ProcFsMountCreator {
    pub fn get() -> Arc<ProcFs> {
        unsafe { GLOBAL_PROCFS.as_ref().cloned().unwrap() }
    }

    pub fn get_weak() -> Weak<ProcFs> {
        unsafe { GLOBAL_PROCFS.as_ref().map(Arc::downgrade).unwrap() }
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

    ProcFsNode(root)
}

pub fn creat(
    parent: &ProcFsNode,
    name: &Arc<[u8]>,
    file: Box<dyn ProcFsFile>,
) -> KResult<ProcFsNode> {
    let mut mode = S_IFREG;
    if file.can_read() {
        mode |= 0o444;
    }
    if file.can_write() {
        mode |= 0o200;
    }

    let dir = parent
        .0
        .ops
        .as_any()
        .downcast_ref::<ProcFsDirectory>()
        .ok_or(ENOTDIR)?;

    let fs = ProcFsMountCreator::get();
    let ino = fs.next_ino.fetch_add(1, Ordering::SeqCst);

    let inode = get_icache().alloc(ino, Box::new(ProcFsFileOps { file }));

    inode.idata.lock().mode = mode;
    inode.idata.lock().nlink = 1;

    dir.entries
        .lock()
        .push((name.clone(), ProcFsNode(inode.clone())));

    Ok(ProcFsNode(inode))
}

pub fn mkdir(parent: &ProcFsNode, name: &[u8]) -> KResult<ProcFsNode> {
    let dir = parent
        .0
        .ops
        .as_any()
        .downcast_ref::<ProcFsDirectory>()
        .ok_or(ENOTDIR)?;

    let ino = ProcFsMountCreator::get()
        .next_ino
        .fetch_add(1, Ordering::SeqCst);

    let inode = get_icache().alloc(
        ino,
        Box::new(ProcFsDirectory {
            entries: Mutex::new(vec![]),
        }),
    );

    {
        let mut idata = inode.idata.lock();
        idata.nlink = 2;
        idata.mode = S_IFDIR | 0o755;
    }

    dir.entries
        .lock()
        .push((Arc::from(name), ProcFsNode(inode.clone())));

    Ok(ProcFsNode(inode))
}

struct DumpMountsFile {}
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
    let dir = ProcFsDirectory {
        entries: Mutex::new(vec![]),
    };

    let fs: Arc<ProcFs> = Arc::new_cyclic(|weak: &Weak<ProcFs>| {
        let root_node = Arc::new(Inode {
            ino: 0,
            vfs: weak.clone(),
            idata: Mutex::new(InodeData::default()),
            ops: Box::new(dir),
        });

        ProcFs {
            root_node,
            next_ino: AtomicIno::new(1),
        }
    });

    {
        let mut indata = fs.root_node.idata.lock();
        indata.mode = S_IFDIR | 0o755;
        indata.nlink = 1;
    };

    unsafe {
        GLOBAL_PROCFS = Some(fs);
        ICACHE = Some(InodeCache::new(ProcFsMountCreator::get_weak()));
    };

    register_filesystem("procfs", Box::new(ProcFsMountCreator)).unwrap();

    let root = root();

    creat(
        &root,
        &Arc::from(b"mounts".as_slice()),
        Box::new(DumpMountsFile {}),
    )
    .unwrap();
}
