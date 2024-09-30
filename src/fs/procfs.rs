use alloc::sync::{Arc, Weak};
use bindings::{EACCES, EINVAL, EISDIR, ENOTDIR, S_IFDIR, S_IFREG};

use crate::{
    io::copy_offset_count,
    kernel::{
        mem::paging::{Page, PageBuffer},
        vfs::{
            inode::{Ino, Inode, InodeData},
            mount::{dump_mounts, register_filesystem, Mount, MountCreator},
            vfs::Vfs,
            DevId, ReadDirCallback, TimeSpec,
        },
    },
    prelude::*,
};

pub trait ProcFsFile: Send + Sync {
    fn can_read(&self) -> bool {
        false
    }

    fn can_write(&self) -> bool {
        false
    }

    fn read(&self, _buffer: &mut PageBuffer) -> KResult<usize> {
        Err(EINVAL)
    }

    fn write(&self, _buffer: &[u8]) -> KResult<usize> {
        Err(EINVAL)
    }
}

pub enum ProcFsData {
    File(Box<dyn ProcFsFile>),
    Directory(Mutex<Vec<Arc<ProcFsNode>>>),
}

pub struct ProcFsNode {
    indata: Mutex<InodeData>,

    name: String,
    data: ProcFsData,
}

impl ProcFsNode {
    fn new(ino: Ino, name: String, data: ProcFsData) -> Self {
        Self {
            indata: Mutex::new(InodeData {
                ino,
                mode: 0,
                uid: 0,
                gid: 0,
                size: 0,
                atime: TimeSpec { sec: 0, nsec: 0 },
                mtime: TimeSpec { sec: 0, nsec: 0 },
                ctime: TimeSpec { sec: 0, nsec: 0 },
                nlink: 0,
            }),
            name,
            data,
        }
    }
}

impl Inode for ProcFsNode {
    fn idata(&self) -> &Mutex<InodeData> {
        &self.indata
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn readdir(
        &self,
        offset: usize,
        callback: &mut ReadDirCallback,
    ) -> KResult<usize> {
        match self.data {
            ProcFsData::Directory(ref lck) => {
                let dir = lck.lock();

                let mut nread = 0;
                for entry in dir.iter().skip(offset) {
                    let inode: Arc<dyn Inode> = entry.clone();
                    callback(
                        entry.name.as_str(),
                        &inode,
                        &entry.indata.lock(),
                        0,
                    )?;

                    nread += 1;
                }

                Ok(nread)
            }
            _ => Err(ENOTDIR),
        }
    }

    fn read(&self, buffer: &mut [u8], offset: usize) -> KResult<usize> {
        match self.data {
            ProcFsData::File(ref file) => {
                if !file.can_read() {
                    return Err(EACCES);
                }

                let mut page_buffer = PageBuffer::new(Page::alloc_one());
                let nread = file.read(&mut page_buffer)?;

                let data = match page_buffer.as_slice().split_at_checked(nread)
                {
                    None => return Ok(0),
                    Some((data, _)) => data,
                };

                Ok(copy_offset_count(data, buffer, offset, buffer.len()))
            }
            _ => Err(EISDIR),
        }
    }

    fn vfs_weak(&self) -> Weak<Mutex<dyn Vfs>> {
        ProcFsMountCreator::get_weak()
    }

    fn vfs_strong(&self) -> Option<Arc<Mutex<dyn Vfs>>> {
        Some(ProcFsMountCreator::get())
    }
}

pub struct ProcFs {
    root_node: Arc<ProcFsNode>,
    next_ino: Ino,
}

impl ProcFs {
    pub fn create() -> Arc<Mutex<Self>> {
        let fs = Arc::new(Mutex::new(Self {
            root_node: Arc::new(ProcFsNode::new(
                0,
                String::from("[root]"),
                ProcFsData::Directory(Mutex::new(vec![])),
            )),
            next_ino: 1,
        }));

        {
            let fs = fs.lock();

            let mut indata = fs.root_node.indata.lock();
            indata.mode = S_IFDIR | 0o755;
            indata.nlink = 1;
        };

        fs
    }
}

impl Vfs for ProcFs {
    fn io_blksize(&self) -> usize {
        1024
    }

    fn fs_devid(&self) -> DevId {
        10
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

static GLOBAL_PROCFS: Mutex<Option<Arc<Mutex<ProcFs>>>> = Mutex::new(None);

struct ProcFsMountCreator;

impl ProcFsMountCreator {
    pub fn get() -> Arc<Mutex<ProcFs>> {
        let fs = GLOBAL_PROCFS.lock();
        fs.as_ref().unwrap().clone()
    }

    pub fn get_weak() -> Weak<Mutex<ProcFs>> {
        let fs = GLOBAL_PROCFS.lock();
        fs.as_ref()
            .map_or(Weak::new(), |refproc| Arc::downgrade(refproc))
    }
}

impl MountCreator for ProcFsMountCreator {
    fn create_mount(
        &self,
        _source: &str,
        _flags: u64,
        _data: &[u8],
    ) -> KResult<Mount> {
        let vfs = ProcFsMountCreator::get();

        let root_inode = vfs.lock().root_node.clone();
        Ok(Mount::new(vfs, root_inode))
    }
}

pub fn root() -> Arc<ProcFsNode> {
    let vfs = ProcFsMountCreator::get();
    let root = vfs.lock().root_node.clone();

    root
}

pub fn creat(
    parent: &ProcFsNode,
    name: &str,
    data: ProcFsData,
) -> KResult<Arc<ProcFsNode>> {
    let mut mode = S_IFREG;
    match data {
        ProcFsData::File(ref file) => {
            if file.can_read() {
                mode |= 0o444;
            }
            if file.can_write() {
                mode |= 0o200;
            }
        }
        _ => return Err(EINVAL),
    }

    match parent.data {
        ProcFsData::Directory(ref lck) => {
            let ino = {
                let fs = ProcFsMountCreator::get();
                let mut fs = fs.lock();

                let ino = fs.next_ino;
                fs.next_ino += 1;

                ino
            };

            let node = Arc::new(ProcFsNode::new(ino, String::from(name), data));

            {
                let mut indata = node.indata.lock();
                indata.nlink = 1;
                indata.mode = mode;
            }

            lck.lock().push(node.clone());

            Ok(node.clone())
        }
        _ => Err(ENOTDIR),
    }
}

pub fn mkdir(parent: &mut ProcFsNode, name: &str) -> KResult<Arc<ProcFsNode>> {
    match parent.data {
        ProcFsData::Directory(ref lck) => {
            let ino = {
                let fs = ProcFsMountCreator::get();
                let mut fs = fs.lock();

                let ino = fs.next_ino;
                fs.next_ino += 1;

                ino
            };

            let node = Arc::new(ProcFsNode::new(
                ino,
                String::from(name),
                ProcFsData::Directory(Mutex::new(vec![])),
            ));

            {
                let mut indata = node.indata.lock();
                indata.nlink = 2;
                indata.mode = S_IFDIR | 0o755;
            }

            lck.lock().push(node.clone());

            Ok(node.clone())
        }
        _ => Err(ENOTDIR),
    }
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
    {
        let mut vfs = GLOBAL_PROCFS.lock();
        *vfs = Some(ProcFs::create());
    }

    register_filesystem("procfs", Box::new(ProcFsMountCreator)).unwrap();

    let root = root();

    creat(
        &root,
        "mounts",
        ProcFsData::File(Box::new(DumpMountsFile {})),
    )
    .unwrap();
}
