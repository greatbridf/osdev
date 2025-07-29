use core::sync::atomic::{AtomicU32, Ordering};

use alloc::{collections::btree_map::BTreeMap, sync::Arc};
use bitflags::bitflags;
use eonix_sync::{LazyLock, Mutex};

use crate::{
    fs::tmpfs::{DirectoryInode, FileInode, TmpFs},
    kernel::{constants::ENOSPC, timer::Instant, vfs::inode::Mode},
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

#[repr(C)]
#[derive(Default, Clone, Copy, Debug)]
pub struct IpcPerm {
    key: i32,
    uid: u32,
    gid: u32,
    cuid: u32,
    cgid: u32,
    mode: u16,
    seq: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ShmIdDs {
    // Ownership and permissions
    pub shm_perm: IpcPerm,
    // Size of segment (bytes). In our system, this must be aligned
    pub shm_segsz: usize,
    // Last attach time
    pub shm_atime: usize,
    // Last detach time
    pub shm_dtime: usize,
    // Creation time/time of last modification via shmctl()
    pub shm_ctime: usize,
    // PID of creator
    pub shm_cpid: usize,
    // PID of last shmat(2)/shmdt(2)
    pub shm_lpid: usize,
    // No. of current attaches
    pub shm_nattch: usize,
}

impl ShmIdDs {
    fn new(size: usize, pid: u32) -> Self {
        Self {
            shm_perm: IpcPerm::default(),
            shm_segsz: size,
            shm_atime: 0,
            shm_dtime: 0,
            shm_ctime: 0, // Should set instant now
            shm_cpid: pid as usize,
            shm_lpid: 0,
            shm_nattch: 0,
        }
    }
}

#[derive(Debug)]
pub struct ShmArea {
    pub area: Arc<FileInode>,
    pub shmid_ds: ShmIdDs,
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

    pub fn create_shared_area(&self, size: usize, pid: u32, mode: Mode) -> ShmArea {
        let ino = self.tmpfs.assign_ino();
        let vfs = Arc::downgrade(&self.tmpfs);
        ShmArea {
            area: FileInode::new(ino, vfs, size, mode),
            shmid_ds: ShmIdDs::new(size, pid),
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

        if shmid >= SHM_MAGIC {
            return Err(ENOSPC);
        } else {
            return Ok(shmid);
        }
    }

    (key as u32).checked_add(SHM_MAGIC).ok_or(ENOSPC)
}
