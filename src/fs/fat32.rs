mod dir;
mod file;

use core::future::Future;
use core::ops::Deref;

use alloc::sync::{Arc, Weak};
use async_trait::async_trait;
use dir::{as_raw_dirents, ParseDirent};
use eonix_sync::RwLock;
use itertools::Itertools;

use crate::kernel::constants::{EINVAL, EIO};
use crate::kernel::mem::{AsMemoryBlock, CachePageStream};
use crate::kernel::timer::Instant;
use crate::kernel::vfs::inode::{InodeDirOps, InodeFileOps, InodeInfo, InodeOps, InodeUse};
use crate::kernel::vfs::types::{DeviceId, Format, Permission};
use crate::kernel::vfs::{SbRef, SbUse, SuperBlock, SuperBlockInfo};
use crate::prelude::*;
use crate::{
    io::{Buffer, ByteBuffer, UninitBuffer},
    kernel::{
        block::{BlockDevice, BlockDeviceRequest},
        mem::{
            paging::Page,
            {CachePage, PageCache, PageCacheBackendOps},
        },
        vfs::{
            dentry::Dentry,
            inode::{Ino, Inode},
            mount::{register_filesystem, Mount, MountCreator},
        },
    },
    KResult,
};

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Cluster(u32);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

    fn normalized(self) -> Self {
        Self(self.0 - 2)
    }
}

const SECTOR_SIZE: usize = 512;

#[derive(Clone, Copy)]
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
    async fn read_cluster(&self, mut cluster: Cluster, buf: &Page) -> KResult<()> {
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
    pub async fn create(device: DeviceId) -> KResult<(SbUse<Self>, InodeUse<dyn Inode>)> {
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

struct FileInode {
    cluster: Cluster,
    info: Spin<InodeInfo>,
    sb: SbRef<FatFs>,
    page_cache: PageCache,
}

impl FileInode {
    fn new(cluster: Cluster, sb: SbRef<FatFs>, size: u32) -> InodeUse<FileInode> {
        InodeUse::new_cyclic(|weak: &Weak<FileInode>| Self {
            cluster,
            info: Spin::new(InodeInfo {
                size: size as u64,
                nlink: 1,
                uid: 0,
                gid: 0,
                perm: Permission::new(0o777),
                atime: Instant::UNIX_EPOCH,
                ctime: Instant::UNIX_EPOCH,
                mtime: Instant::UNIX_EPOCH,
            }),
            sb,
            page_cache: PageCache::new(weak.clone()),
        })
    }
}

impl InodeOps for FileInode {
    type SuperBlock = FatFs;

    fn ino(&self) -> Ino {
        self.cluster.as_ino()
    }

    fn format(&self) -> Format {
        Format::REG
    }

    fn info(&self) -> &Spin<InodeInfo> {
        &self.info
    }

    fn super_block(&self) -> &SbRef<Self::SuperBlock> {
        &self.sb
    }

    fn page_cache(&self) -> Option<&PageCache> {
        Some(&self.page_cache)
    }
}

impl InodeDirOps for FileInode {}
impl InodeFileOps for FileInode {
    async fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        self.page_cache.read(buffer, offset).await
    }

    async fn read_direct(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        let sb = self.sb.get()?;
        let fs = &sb.backend;
        let fat = sb.backend.fat.read().await;

        if offset >= self.info.lock().size as usize {
            return Ok(0);
        }

        let cluster_size = fs.sectors_per_cluster as usize * SECTOR_SIZE;
        assert!(cluster_size <= 0x1000, "Cluster size is too large");

        let skip_clusters = offset / cluster_size;
        let inner_offset = offset % cluster_size;

        let cluster_iter = ClusterIterator::new(fat.as_ref(), self.cluster).skip(skip_clusters);

        let buffer_page = Page::alloc();
        for cluster in cluster_iter {
            fs.read_cluster(cluster, &buffer_page).await?;

            let data = unsafe {
                // SAFETY: We are the only one holding this page.
                &buffer_page.as_memblk().as_bytes()[inner_offset..]
            };

            let end = offset + data.len();
            let real_end = end.min(self.info.lock().size as usize);
            let real_size = real_end - offset;

            if buffer.fill(&data[..real_size])?.should_stop() {
                break;
            }
        }

        Ok(buffer.wrote())
    }
}

impl PageCacheBackendOps for FileInode {
    async fn read_page(&self, page: &mut CachePage, offset: usize) -> KResult<usize> {
        self.read_direct(page, offset).await
    }

    async fn write_page(&self, _page: &mut CachePageStream, _offset: usize) -> KResult<usize> {
        todo!()
    }

    fn size(&self) -> usize {
        self.info.lock().size as usize
    }
}

struct DirInode {
    cluster: Cluster,
    info: Spin<InodeInfo>,
    sb: SbRef<FatFs>,

    // TODO: Use the new PageCache...
    dir_pages: RwLock<Vec<Page>>,
}

impl DirInode {
    fn new(cluster: Cluster, sb: SbRef<FatFs>, size: u32) -> InodeUse<Self> {
        InodeUse::new(Self {
            cluster,
            info: Spin::new(InodeInfo {
                size: size as u64,
                nlink: 2, // '.' and '..'
                uid: 0,
                gid: 0,
                perm: Permission::new(0o777),
                atime: Instant::UNIX_EPOCH,
                ctime: Instant::UNIX_EPOCH,
                mtime: Instant::UNIX_EPOCH,
            }),
            sb,
            dir_pages: RwLock::new(Vec::new()),
        })
    }

    async fn read_dir_pages(&self) -> KResult<()> {
        let mut dir_pages = self.dir_pages.write().await;
        if !dir_pages.is_empty() {
            return Ok(());
        }

        let sb = self.sb.get()?;
        let fs = &sb.backend;
        let fat = fs.fat.read().await;

        let clusters = ClusterIterator::new(fat.as_ref(), self.cluster);

        for cluster in clusters {
            let page = Page::alloc();
            fs.read_cluster(cluster, &page).await?;

            dir_pages.push(page);
        }

        Ok(())
    }

    async fn get_dir_pages(&self) -> KResult<impl Deref<Target = Vec<Page>> + use<'_>> {
        {
            let dir_pages = self.dir_pages.read().await;
            if !dir_pages.is_empty() {
                return Ok(dir_pages);
            }
        }

        self.read_dir_pages().await?;

        if let Some(dir_pages) = self.dir_pages.try_read() {
            return Ok(dir_pages);
        }

        Ok(self.dir_pages.read().await)
    }
}

impl InodeOps for DirInode {
    type SuperBlock = FatFs;

    fn ino(&self) -> Ino {
        self.cluster.as_ino()
    }

    fn format(&self) -> Format {
        Format::DIR
    }

    fn info(&self) -> &Spin<InodeInfo> {
        &self.info
    }

    fn super_block(&self) -> &SbRef<Self::SuperBlock> {
        &self.sb
    }

    fn page_cache(&self) -> Option<&PageCache> {
        None
    }
}

impl InodeFileOps for DirInode {}
impl InodeDirOps for DirInode {
    async fn lookup(&self, dentry: &Arc<Dentry>) -> KResult<Option<InodeUse<dyn Inode>>> {
        let sb = self.sb.get()?;
        let dir_pages = self.get_dir_pages().await?;

        let dir_data = dir_pages.iter().map(|page| {
            unsafe {
                // SAFETY: No one could be writing to it.
                page.as_memblk().as_bytes()
            }
        });

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

    fn readdir<'r, 'a: 'r, 'b: 'r>(
        &'a self,
        offset: usize,
        callback: &'b mut (dyn FnMut(&[u8], Ino) -> KResult<bool> + Send),
    ) -> impl Future<Output = KResult<KResult<usize>>> + Send + 'r {
        async move {
            let sb = self.sb.get()?;
            let fs = &sb.backend;
            let dir_pages = self.get_dir_pages().await?;

            let cluster_size = fs.sectors_per_cluster as usize * SECTOR_SIZE;

            let cluster_offset = offset / cluster_size;
            let inner_offset = offset % cluster_size;
            let inner_raw_dirent_offset = inner_offset / core::mem::size_of::<dir::RawDirEntry>();

            let dir_data = dir_pages.iter().skip(cluster_offset).map(|page| {
                unsafe {
                    // SAFETY: No one could be writing to it.
                    page.as_memblk().as_bytes()
                }
            });

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
