use crate::kernel::constants::{EACCES, EISDIR, ENOTDIR};
use crate::kernel::timer::Instant;
use crate::kernel::vfs::inode::{InodeDirOps, InodeFileOps, InodeInfo, InodeOps, InodeUse};
use crate::kernel::vfs::types::{DeviceId, Format, Permission};
use crate::kernel::vfs::{SbRef, SbUse, SuperBlock, SuperBlockInfo};
use crate::{
    io::Buffer,
    kernel::{
        mem::paging::PageBuffer,
        vfs::{
            dentry::Dentry,
            inode::{Ino, Inode},
            mount::{dump_mounts, register_filesystem, Mount, MountCreator},
        },
    },
    prelude::*,
};
use alloc::sync::Arc;
use async_trait::async_trait;
use core::future::Future;
use core::sync::atomic::{AtomicU64, Ordering};
use eonix_sync::{LazyLock, RwLock};

struct Node {
    ino: Ino,
    sb: SbRef<ProcFs>,
    info: Spin<InodeInfo>,
    kind: NodeKind,
}

enum NodeKind {
    File(FileInode),
    Dir(DirInode),
}

struct FileInode {
    read: Option<Box<dyn Fn(&mut PageBuffer) -> KResult<()> + Send + Sync>>,
    write: Option<()>,
}

struct DirInode {
    entries: RwLock<Vec<(Arc<[u8]>, InodeUse<Node>)>>,
}

impl InodeOps for Node {
    type SuperBlock = ProcFs;

    fn ino(&self) -> Ino {
        self.ino
    }

    fn format(&self) -> Format {
        match &self.kind {
            NodeKind::File(_) => Format::REG,
            NodeKind::Dir(_) => Format::DIR,
        }
    }

    fn info(&self) -> &Spin<InodeInfo> {
        &self.info
    }

    fn super_block(&self) -> &SbRef<Self::SuperBlock> {
        &self.sb
    }

    fn page_cache(&self) -> Option<&crate::kernel::mem::PageCache> {
        None
    }
}

impl InodeFileOps for Node {
    async fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        let NodeKind::File(file_inode) = &self.kind else {
            return Err(EISDIR);
        };

        let Some(read_fn) = &file_inode.read else {
            return Err(EACCES);
        };

        let mut page_buffer = PageBuffer::new();
        read_fn(&mut page_buffer)?;

        let Some((_, data)) = page_buffer.data().split_at_checked(offset) else {
            return Ok(0);
        };

        Ok(buffer.fill(data)?.allow_partial())
    }
}

impl InodeDirOps for Node {
    async fn lookup(&self, dentry: &Arc<Dentry>) -> KResult<Option<InodeUse<dyn Inode>>> {
        let NodeKind::Dir(dir) = &self.kind else {
            return Err(ENOTDIR);
        };

        let entries = dir.entries.read().await;

        let dent_name = dentry.name();
        for (name, node) in entries.iter() {
            if *name == ***dent_name {
                return Ok(Some(node.clone() as _));
            }
        }

        Ok(None)
    }

    fn readdir<'r, 'a: 'r, 'b: 'r>(
        &'a self,
        offset: usize,
        callback: &'b mut (dyn FnMut(&[u8], Ino) -> KResult<bool> + Send),
    ) -> impl Future<Output = KResult<KResult<usize>>> + Send + 'r {
        Box::pin(async move {
            let NodeKind::Dir(dir) = &self.kind else {
                return Err(ENOTDIR);
            };

            let entries = dir.entries.read().await;

            let mut count = 0;
            for (name, node) in entries.iter().skip(offset) {
                match callback(name.as_ref(), node.ino) {
                    Err(err) => return Ok(Err(err)),
                    Ok(true) => count += 1,
                    Ok(false) => break,
                }
            }

            Ok(Ok(count))
        })
    }
}

impl Node {
    pub fn new_file(
        ino: Ino,
        sb: SbRef<ProcFs>,
        read: impl Fn(&mut PageBuffer) -> KResult<()> + Send + Sync + 'static,
    ) -> InodeUse<Self> {
        InodeUse::new(Self {
            ino,
            sb,
            info: Spin::new(InodeInfo {
                size: 0,
                nlink: 1,
                uid: 0,
                gid: 0,
                perm: Permission::new(0o444),
                atime: Instant::UNIX_EPOCH,
                ctime: Instant::UNIX_EPOCH,
                mtime: Instant::UNIX_EPOCH,
            }),
            kind: NodeKind::File(FileInode::new(Box::new(read))),
        })
    }

    fn new_dir(ino: Ino, sb: SbRef<ProcFs>) -> InodeUse<Self> {
        InodeUse::new(Self {
            ino,
            sb,
            info: Spin::new(InodeInfo {
                size: 0,
                nlink: 1,
                uid: 0,
                gid: 0,
                perm: Permission::new(0o755),
                atime: Instant::UNIX_EPOCH,
                ctime: Instant::UNIX_EPOCH,
                mtime: Instant::UNIX_EPOCH,
            }),
            kind: NodeKind::Dir(DirInode::new()),
        })
    }
}

impl FileInode {
    fn new(read: Box<dyn Fn(&mut PageBuffer) -> KResult<()> + Send + Sync>) -> Self {
        Self {
            read: Some(read),
            write: None,
        }
    }
}

impl DirInode {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(vec![]),
        }
    }
}

pub struct ProcFs {
    root: InodeUse<Node>,
    next_ino: AtomicU64,
}

impl SuperBlock for ProcFs {}
impl ProcFs {
    fn assign_ino(&self) -> Ino {
        Ino::new(self.next_ino.fetch_add(1, Ordering::Relaxed))
    }
}

static GLOBAL_PROCFS: LazyLock<SbUse<ProcFs>> = LazyLock::new(|| {
    SbUse::new_cyclic(
        SuperBlockInfo {
            io_blksize: 4096,
            device_id: DeviceId::new(0, 10),
            read_only: false,
        },
        |sbref| ProcFs {
            root: Node::new_dir(Ino::new(0), sbref),
            next_ino: AtomicU64::new(1),
        },
    )
});

struct ProcFsMountCreator;

#[async_trait]
impl MountCreator for ProcFsMountCreator {
    async fn create_mount(&self, _source: &str, _flags: u64, mp: &Arc<Dentry>) -> KResult<Mount> {
        let fs = GLOBAL_PROCFS.clone();
        let root_inode = fs.backend.root.clone();

        Mount::new(mp, fs, root_inode)
    }

    fn check_signature(&self, _: &[u8]) -> KResult<bool> {
        Ok(true)
    }
}

pub async fn populate_root<F>(name: Arc<[u8]>, read_fn: F)
where
    F: Send + Sync + Fn(&mut PageBuffer) -> KResult<()> + 'static,
{
    let procfs = &GLOBAL_PROCFS.backend;
    let root = &procfs.root;

    let NodeKind::Dir(root) = &root.kind else {
        unreachable!();
    };

    let mut entries = root.entries.write().await;
    entries.push((
        name.clone(),
        Node::new_file(procfs.assign_ino(), SbRef::from(&GLOBAL_PROCFS), read_fn),
    ));
}

pub async fn init() {
    register_filesystem("procfs", Arc::new(ProcFsMountCreator)).unwrap();

    populate_root(Arc::from(b"mounts".as_slice()), |buffer| {
        dump_mounts(&mut buffer.get_writer());
        Ok(())
    })
    .await;
}
