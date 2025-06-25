use core::sync::atomic::{AtomicU32, Ordering};

use alloc::{collections::btree_map::BTreeMap, sync::Arc};
use bitflags::bitflags;
use eonix_sync::{LazyLock, Mutex};

use crate::{
    fs::tmpfs::{DirectoryInode, FileInode, TmpFs},
    kernel::{constants::ENOSPC, vfs::inode::Mode},
    prelude::KResult,
};

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct ShmFlags: u32 {
        /// Create a new segment. If this flag is not used, then shmget() will
        /// find the segment associated with key and check to see if the user
        /// has permission to access the segment.
        const IPC_CREAT = 0o1000;
        /// This flag is used with IPC_CREAT to ensure that this call creates
        /// the segment.  If the segment already exists, the call fails.
        const IPC_EXCL = 0o2000;

        /// Attach the segment for read-only access.If this flag is not specified,
        /// the segment is attached for read and write access, and the process
        /// must have read and write permission for the segment.
        const SHM_RDONLY = 0o10000;
        /// round attach address to SHMLBA boundary
        const SHM_RND = 0o20000;
        /// Allow the contents of the segment to be executed.
        const SHM_EXEC = 0o100000;
    }
}

pub const IPC_PRIVATE: usize = 0;

pub struct ShmManager {
    tmpfs: Arc<TmpFs>,
    root: Arc<DirectoryInode>,
    areas: BTreeMap<u32, ShmArea>,
}

pub struct ShmArea {
    pub area: Arc<FileInode>,
    pub size: usize,
}

// A big lock here to protect the shared memory area.
// Can be improved with finer-grained locking?
pub static SHM_MANAGER: LazyLock<Mutex<ShmManager>> =
    LazyLock::new(|| Mutex::new(ShmManager::new()));

impl ShmManager {
    fn new() -> Self {
        let (tmpfs, root) = TmpFs::create(false).expect("should create shm_area successfully");
        Self {
            tmpfs,
            root,
            areas: BTreeMap::new(),
        }
    }

    pub fn create_shared_area(&self, size: usize, mode: Mode) -> ShmArea {
        let ino = self.tmpfs.assign_ino();
        let vfs = Arc::downgrade(&self.tmpfs);
        ShmArea {
            area: FileInode::new(ino, vfs, mode),
            size,
        }
    }

    pub fn get(&self, shmid: u32) -> Option<&ShmArea> {
        self.areas.get(&shmid)
    }

    pub fn insert(&mut self, shmid: u32, area: ShmArea) {
        self.areas.insert(shmid, area);
    }
}

pub fn gen_shm_id(key: usize) -> KResult<u32> {
    const SHM_MAGIC: u32 = 114514000;

    static NEXT_SHMID: AtomicU32 = AtomicU32::new(0);

    if key == IPC_PRIVATE {
        let shmid = NEXT_SHMID.fetch_add(1, Ordering::Relaxed);

        if shmid < SHM_MAGIC {
            return Err(ENOSPC);
        } else {
            return Ok(shmid);
        }
    }

    (key as u32).checked_add(SHM_MAGIC).ok_or(ENOSPC)
}
