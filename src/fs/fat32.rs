mod dir;

use alloc::sync::Arc;
use core::ops::Deref;

use async_trait::async_trait;
use dir::{as_raw_dirents, ParseDirent};
use eonix_mm::paging::PAGE_SIZE;
use eonix_sync::RwLock;
use itertools::Itertools;

use crate::io::{Buffer, ByteBuffer, UninitBuffer};
use crate::kernel::block::{BlockDevice, BlockDeviceRequest};
use crate::kernel::constants::{EINVAL, EIO};
use crate::kernel::mem::{CachePage, Folio, FolioOwned, PageOffset};
use crate::kernel::timer::Instant;
use crate::kernel::vfs::dentry::Dentry;
use crate::kernel::vfs::inode::{Ino, InodeInfo, InodeOps, InodeUse};
use crate::kernel::vfs::mount::{register_filesystem, Mount, MountCreator};
use crate::kernel::vfs::types::{DeviceId, Format, Permission};
use crate::kernel::vfs::{SbRef, SbUse, SuperBlock, SuperBlockInfo};
use crate::prelude::*;
use crate::KResult;

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Cluster(u32);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct RawCluster(pub u32);

impl RawCluster {
    const START: u32 = 2;
    const EOC: u32 = 0x0FFF_FFF8;
    const INVL: u32 = 0xF000_0000;

    fn parse(self) -> Option<Cluster> {
        match self.0 {
            ..Self::START | Self::EOC..Self::INVL => None,
            Self::INVL.. => {
                unreachable!("invalid cluster number: RawCluster({:#08x})", self.0)
            }
            no => Some(Cluster(no)),
        }
    }
}

impl Cluster {
    pub fn as_ino(self) -> Ino {
        Ino::new(self.0 as _)
    }

    pub fn from_ino(ino: Ino) -> Self {
        Self(ino.as_raw() as u32)
    }

    fn normalized(self) -> Self {
        Self(self.0 - 2)
    }
}

const SECTOR_SIZE: usize = 512;

#[derive(Clone, Copy, Debug)]
#[repr(C, packed)]
struct Bootsector {
    jmp: [u8; 3],
    oem: [u8; 8],
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    fat_copies: u8,
    root_entries: u16,   // should be 0 for FAT32
    _total_sectors: u16, // outdated
    media: u8,
    _sectors_per_fat: u16, // outdated
    sectors_per_track: u16,
    heads: u16,
    hidden_sectors: u32,
    total_sectors: u32,
    sectors_per_fat: u32,
    flags: u16,
    fat_version: u16,
    root_cluster: RawCluster,
    fsinfo_sector: u16,
    backup_bootsector: u16,
    _reserved: [u8; 12],
    drive_number: u8,
    _reserved2: u8,
    ext_sig: u8,
    serial: u32,
    volume_label: [u8; 11],
    fs_type: [u8; 8],
    bootcode: [u8; 420],
    mbr_signature: u16,
}

/// # Lock order
/// 2. FatTable
/// 3. Inodes
///
struct FatFs {
    sectors_per_cluster: u8,
    data_start_sector: u64,
    _rootdir_cluster: Cluster,
    _volume_label: Box<str>,

    device: Arc<BlockDevice>,
    fat: RwLock<Box<[RawCluster]>>,
}

impl SuperBlock for FatFs {}

impl FatFs {
    async fn read_cluster(&self, mut cluster: Cluster, buf: &Folio) -> KResult<()> {
        cluster = cluster.normalized();

        let rq = BlockDeviceRequest::Read {
            sector: self.data_start_sector as u64
                + cluster.0 as u64 * self.sectors_per_cluster as u64,
            count: self.sectors_per_cluster as u64,
            buffer: core::slice::from_ref(buf),
        };

        self.device.commit_request(rq).await?;
        Ok(())
    }
}

impl FatFs {
    pub async fn create(device: DeviceId) -> KResult<(SbUse<Self>, InodeUse)> {
        let device = BlockDevice::get(device)?;

        let mut info = UninitBuffer::<Bootsector>::new();
        device.read_some(0, &mut info).await?.ok_or(EIO)?;
        let info = info.assume_filled_ref()?;

        let mut fat = Box::new_uninit_slice(
            512 * info.sectors_per_fat as usize / core::mem::size_of::<Cluster>(),
        );

        device
            .read_some(
                info.reserved_sectors as usize * 512,
                &mut ByteBuffer::from(fat.as_mut()),
            )
            .await?
            .ok_or(EIO)?;

        let sectors_per_cluster = info.sectors_per_cluster;
        let rootdir_cluster = info.root_cluster.parse().ok_or(EINVAL)?;

        let data_start_sector =
            info.reserved_sectors as u64 + info.fat_copies as u64 * info.sectors_per_fat as u64;

        let volume_label = {
            let end = info
                .volume_label
                .iter()
                .position(|&c| c == b' ')
                .unwrap_or(info.volume_label.len());

            String::from_utf8_lossy(&info.volume_label[..end])
                .into_owned()
                .into_boxed_str()
        };

        let fat = unsafe { fat.assume_init() };

        let rootdir_cluster_count = ClusterIterator::new(fat.as_ref(), rootdir_cluster).count();
        let rootdir_size = rootdir_cluster_count as u32 * sectors_per_cluster as u32 * 512;

        let fatfs = SbUse::new(
            SuperBlockInfo {
                io_blksize: 4096,
                device_id: device.devid(),
                read_only: true,
            },
            Self {
                device,
                sectors_per_cluster,
                _rootdir_cluster: rootdir_cluster,
                data_start_sector,
                fat: RwLock::new(fat),
                _volume_label: volume_label,
            },
        );

        let sbref = SbRef::from(&fatfs);
        Ok((fatfs, DirInode::new(rootdir_cluster, sbref, rootdir_size)))
    }
}

struct ClusterIterator<'a> {
    fat: &'a [RawCluster],
    cur: Option<Cluster>,
}

impl<'a> ClusterIterator<'a> {
    fn new(fat: &'a [RawCluster], start: Cluster) -> Self {
        Self {
            fat,
            cur: Some(start),
        }
    }
}

impl<'fat> Iterator for ClusterIterator<'fat> {
    type Item = Cluster;

    fn next(&mut self) -> Option<Self::Item> {
        self.cur.inspect(|&Cluster(no)| {
            self.cur = self.fat[no as usize].parse();
        })
    }
}

struct FileInode;

impl FileInode {
    fn new(cluster: Cluster, sb: SbRef<FatFs>, size: u32) -> InodeUse {
        InodeUse::new(
            sb,
            cluster.as_ino(),
            Format::REG,
            InodeInfo {
                size: size as u64,
                nlink: 1,
                uid: 0,
                gid: 0,
                perm: Permission::new(0o777),
                atime: Instant::UNIX_EPOCH,
                ctime: Instant::UNIX_EPOCH,
                mtime: Instant::UNIX_EPOCH,
            },
            Self,
        )
    }
}

impl InodeOps for FileInode {
    type SuperBlock = FatFs;

    async fn read(
        &self,
        _: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        buffer: &mut dyn Buffer,
        offset: usize,
    ) -> KResult<usize> {
        inode.get_page_cache().read(buffer, offset).await
    }

    async fn read_page(
        &self,
        sb: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        page: &mut CachePage,
        offset: PageOffset,
    ) -> KResult<()> {
        let fs = &sb.backend;
        let fat = sb.backend.fat.read().await;

        if offset >= PageOffset::from_byte_ceil(inode.info.lock().size as usize) {
            unreachable!("read_page called with offset beyond file size");
        }

        let cluster_size = fs.sectors_per_cluster as usize * SECTOR_SIZE;
        if cluster_size != PAGE_SIZE {
            unimplemented!("cluster size != PAGE_SIZE");
        }

        // XXX: Ugly and inefficient O(n^2) algorithm for sequential file read.
        let cluster = ClusterIterator::new(fat.as_ref(), Cluster::from_ino(inode.ino))
            .skip(offset.page_count())
            .next()
            .ok_or(EIO)?;

        fs.read_cluster(cluster, &page).await?;

        let real_len = (inode.info.lock().size as usize) - offset.byte_count();
        if real_len < PAGE_SIZE {
            let mut page = page.lock();
            page.as_bytes_mut()[real_len..].fill(0);
        }

        Ok(())
    }
}

struct DirInode {
    // TODO: Use the new PageCache...
    dir_pages: RwLock<Vec<FolioOwned>>,
}

impl DirInode {
    fn new(cluster: Cluster, sb: SbRef<FatFs>, size: u32) -> InodeUse {
        InodeUse::new(
            sb,
            cluster.as_ino(),
            Format::DIR,
            InodeInfo {
                size: size as u64,
                nlink: 2, // '.' and '..'
                uid: 0,
                gid: 0,
                perm: Permission::new(0o777),
                atime: Instant::UNIX_EPOCH,
                ctime: Instant::UNIX_EPOCH,
                mtime: Instant::UNIX_EPOCH,
            },
            Self {
                dir_pages: RwLock::new(Vec::new()),
            },
        )
    }

    async fn read_dir_pages(&self, sb: &SbUse<FatFs>, inode: &InodeUse) -> KResult<()> {
        let mut dir_pages = self.dir_pages.write().await;
        if !dir_pages.is_empty() {
            return Ok(());
        }

        let fs = &sb.backend;
        let fat = fs.fat.read().await;

        let clusters = ClusterIterator::new(fat.as_ref(), Cluster::from_ino(inode.ino));

        for cluster in clusters {
            let page = FolioOwned::alloc();
            fs.read_cluster(cluster, &page).await?;

            dir_pages.push(page);
        }

        Ok(())
    }

    async fn get_dir_pages(
        &self,
        sb: &SbUse<FatFs>,
        inode: &InodeUse,
    ) -> KResult<impl Deref<Target = Vec<FolioOwned>> + use<'_>> {
        {
            let dir_pages = self.dir_pages.read().await;
            if !dir_pages.is_empty() {
                return Ok(dir_pages);
            }
        }

        self.read_dir_pages(sb, inode).await?;

        if let Some(dir_pages) = self.dir_pages.try_read() {
            return Ok(dir_pages);
        }

        Ok(self.dir_pages.read().await)
    }
}

impl InodeOps for DirInode {
    type SuperBlock = FatFs;

    async fn lookup(
        &self,
        sb: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        dentry: &Arc<Dentry>,
    ) -> KResult<Option<InodeUse>> {
        let dir_pages = self.get_dir_pages(&sb, inode).await?;

        let dir_data = dir_pages.iter().map(|pg| pg.as_bytes());

        let raw_dirents = dir_data
            .map(as_raw_dirents)
            .take_while_inclusive(Result::is_ok)
            .flatten_ok();

        let mut dirents = futures::stream::iter(raw_dirents);

        while let Some(result) = dirents.next_dirent().await {
            let entry = result?;

            if *entry.filename != ****dentry.name() {
                continue;
            }

            let sbref = SbRef::from(&sb);

            if entry.is_directory {
                return Ok(Some(DirInode::new(entry.cluster, sbref, entry.size) as _));
            } else {
                return Ok(Some(FileInode::new(entry.cluster, sbref, entry.size) as _));
            }
        }

        Ok(None)
    }

    async fn readdir(
        &self,
        sb: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        offset: usize,
        callback: &mut (dyn FnMut(&[u8], Ino) -> KResult<bool> + Send),
    ) -> KResult<KResult<usize>> {
        let fs = &sb.backend;
        let dir_pages = self.get_dir_pages(&sb, inode).await?;

        let cluster_size = fs.sectors_per_cluster as usize * SECTOR_SIZE;

        let cluster_offset = offset / cluster_size;
        let inner_offset = offset % cluster_size;
        let inner_raw_dirent_offset = inner_offset / core::mem::size_of::<dir::RawDirEntry>();

        let dir_data = dir_pages
            .iter()
            .skip(cluster_offset)
            .map(|pg| pg.as_bytes());

        let raw_dirents = dir_data
            .map(as_raw_dirents)
            .take_while_inclusive(Result::is_ok)
            .flatten_ok()
            .skip(inner_raw_dirent_offset);

        let mut dirents = futures::stream::iter(raw_dirents);

        let mut nread = 0;
        while let Some(result) = dirents.next_dirent().await {
            let entry = result?;

            match callback(&entry.filename, entry.cluster.as_ino()) {
                Err(err) => return Ok(Err(err)),
                Ok(true) => nread += entry.entry_offset as usize,
                Ok(false) => break,
            }
        }

        Ok(Ok(nread))
    }
}

struct FatMountCreator;

#[async_trait]
impl MountCreator for FatMountCreator {
    fn check_signature(&self, mut first_block: &[u8]) -> KResult<bool> {
        match first_block.split_off(82..) {
            Some([b'F', b'A', b'T', b'3', b'2', b' ', b' ', b' ', ..]) => Ok(true),
            Some(..) => Ok(false),
            None => Err(EIO),
        }
    }

    async fn create_mount(&self, _source: &str, _flags: u64, mp: &Arc<Dentry>) -> KResult<Mount> {
        let (fatfs, root_inode) = FatFs::create(DeviceId::new(8, 1)).await?;

        Mount::new(mp, fatfs, root_inode)
    }
}

pub fn init() {
    register_filesystem("fat32", Arc::new(FatMountCreator)).unwrap();
}
