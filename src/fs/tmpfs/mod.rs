mod dir;
mod file;

use crate::kernel::vfs::inode::{Ino, InodeUse};
use crate::kernel::vfs::types::{DeviceId, Permission};
use crate::kernel::vfs::{SbRef, SbUse, SuperBlock, SuperBlockInfo};
use crate::{
    kernel::vfs::{
        dentry::Dentry,
        mount::{register_filesystem, Mount, MountCreator},
    },
    prelude::*,
};
use alloc::sync::Arc;
use async_trait::async_trait;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use dir::DirectoryInode;
use eonix_sync::Mutex;

pub struct TmpFs {
    next_ino: AtomicU64,
    rename_lock: Mutex<()>,
}

impl SuperBlock for TmpFs {}

impl TmpFs {
    fn assign_ino(&self) -> Ino {
        Ino::new(self.next_ino.fetch_add(1, Ordering::Relaxed))
    }

    fn create() -> KResult<(SbUse<TmpFs>, InodeUse<DirectoryInode>)> {
        let tmpfs = SbUse::new(
            SuperBlockInfo {
                io_blksize: 4096,
                device_id: DeviceId::new(0, 2),
                read_only: false,
            },
            Self {
                next_ino: AtomicU64::new(1),
                rename_lock: Mutex::new(()),
            },
        );

        let root_dir = DirectoryInode::new(
            tmpfs.backend.assign_ino(),
            SbRef::from(&tmpfs),
            Permission::new(0o755),
        );

        Ok((tmpfs, root_dir))
    }
}

struct TmpFsMountCreator;

#[async_trait]
impl MountCreator for TmpFsMountCreator {
    async fn create_mount(&self, _source: &str, _flags: u64, mp: &Arc<Dentry>) -> KResult<Mount> {
        let (fs, root_inode) = TmpFs::create()?;

        Mount::new(mp, fs, root_inode)
    }

    fn check_signature(&self, _: &[u8]) -> KResult<bool> {
        Ok(true)
    }
}

pub fn init() {
    register_filesystem("tmpfs", Arc::new(TmpFsMountCreator)).unwrap();
}
