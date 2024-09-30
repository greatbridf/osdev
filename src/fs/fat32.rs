use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use bindings::{EINVAL, EIO, S_IFDIR, S_IFREG};

use crate::{
    io::{RawBuffer, UninitBuffer},
    kernel::{
        block::{make_device, BlockDevice, BlockDeviceRequest},
        mem::{paging::Page, phys::PhysPtr},
        vfs::{
            inode::{Ino, Inode, InodeCache, InodeData},
            mount::{register_filesystem, Mount, MountCreator},
            vfs::Vfs,
            DevId, ReadDirCallback, TimeSpec,
        },
    },
    prelude::*,
    KResult,
};

type ClusterNo = u32;

const ATTR_RO: u8 = 0x01;
const ATTR_HIDDEN: u8 = 0x02;
const ATTR_SYSTEM: u8 = 0x04;
const ATTR_VOLUME_ID: u8 = 0x08;
const ATTR_DIRECTORY: u8 = 0x10;
const ATTR_ARCHIVE: u8 = 0x20;

const RESERVED_FILENAME_LOWERCASE: u8 = 0x08;

#[repr(C, packed)]
struct FatDirectoryEntry {
    name: [u8; 8],
    extension: [u8; 3],
    attr: u8,
    reserved: u8,
    create_time_tenth: u8,
    create_time: u16,
    create_date: u16,
    access_date: u16,
    cluster_high: u16,
    modify_time: u16,
    modify_date: u16,
    cluster_low: u16,
    size: u32,
}

impl FatDirectoryEntry {
    pub fn filename(&self) -> KResult<String> {
        let basename = str::from_utf8(&self.name)
            .map_err(|_| EINVAL)?
            .trim_end_matches(char::from(' '));

        let extension = if self.extension[0] != ' ' as u8 {
            Some(
                str::from_utf8(&self.extension)
                    .map_err(|_| EINVAL)?
                    .trim_end_matches(char::from(' ')),
            )
        } else {
            None
        };

        let mut name = String::from(basename);

        if let Some(extension) = extension {
            name.push('.');
            name += extension;
        }

        if self.reserved & RESERVED_FILENAME_LOWERCASE != 0 {
            name.make_ascii_lowercase();
        }

        Ok(name)
    }

    pub fn ino(&self) -> Ino {
        let cluster_high = (self.cluster_high as u32) << 16;
        (self.cluster_low as u32 | cluster_high) as Ino
    }

    fn is_volume_id(&self) -> bool {
        self.attr & ATTR_VOLUME_ID != 0
    }

    fn is_free(&self) -> bool {
        self.name[0] == 0x00
    }

    fn is_deleted(&self) -> bool {
        self.name[0] == 0xE5
    }

    fn is_invalid(&self) -> bool {
        self.is_volume_id() || self.is_free() || self.is_deleted()
    }

    fn is_directory(&self) -> bool {
        self.attr & ATTR_DIRECTORY != 0
    }
}

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
    root_cluster: ClusterNo,
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
/// 1. FatFs
/// 2. FatTable
/// 3. Inodes
///
struct FatFs {
    device: Arc<BlockDevice>,
    icache: Mutex<InodeCache<FatFs>>,
    sectors_per_cluster: u8,
    rootdir_cluster: ClusterNo,
    data_start: u64,
    fat: Mutex<Vec<ClusterNo>>,
    volume_label: String,
}

impl FatFs {
    fn read_cluster(&self, cluster: ClusterNo, buf: &Page) -> KResult<()> {
        let cluster = cluster - 2;

        let rq = BlockDeviceRequest {
            sector: self.data_start as u64
                + cluster as u64 * self.sectors_per_cluster as u64,
            count: self.sectors_per_cluster as u64,
            buffer: core::slice::from_ref(buf),
        };
        self.device.read_raw(rq)?;

        Ok(())
    }
}

impl FatFs {
    pub fn create(
        device: DevId,
    ) -> KResult<(Arc<Mutex<Self>>, Arc<dyn Inode>)> {
        let mut fatfs = Self {
            device: BlockDevice::get(device)?,
            icache: Mutex::new(InodeCache::new()),
            sectors_per_cluster: 0,
            rootdir_cluster: 0,
            data_start: 0,
            fat: Mutex::new(Vec::new()),
            volume_label: String::new(),
        };

        let mut info: UninitBuffer<Bootsector> = UninitBuffer::new();
        fatfs.device.read_some(0, &mut info)?.ok_or(EIO)?;
        let info = info.assume_filled_ref()?;

        fatfs.sectors_per_cluster = info.sectors_per_cluster;
        fatfs.rootdir_cluster = info.root_cluster;
        fatfs.data_start = info.reserved_sectors as u64
            + info.fat_copies as u64 * info.sectors_per_fat as u64;

        {
            let mut fat = fatfs.fat.lock();
            fat.resize(
                512 * info.sectors_per_fat as usize
                    / core::mem::size_of::<ClusterNo>(),
                0,
            );

            let mut buffer = RawBuffer::new_from_slice(fat.as_mut_slice());

            fatfs
                .device
                .read_some(info.reserved_sectors as usize * 512, &mut buffer)?
                .ok_or(EIO)?;

            assert!(buffer.filled());
        }

        fatfs.volume_label = String::from(
            str::from_utf8(&info.volume_label)
                .map_err(|_| EINVAL)?
                .trim_end_matches(char::from(' ')),
        );

        let root_dir_cluster_count = {
            let fat = fatfs.fat.lock();

            ClusterIterator::new(&fat, fatfs.rootdir_cluster).count()
        };

        let fatfs = Arc::new(Mutex::new(fatfs));
        let root_inode = {
            let _fatfs = fatfs.lock();
            let mut icache = _fatfs.icache.lock();

            icache.set_vfs(Arc::downgrade(&fatfs));
            let root_inode = FatInode {
                idata: Mutex::new(InodeData {
                    ino: info.root_cluster as Ino,
                    mode: S_IFDIR | 0o777,
                    nlink: 2,
                    size: root_dir_cluster_count as u64
                        * info.sectors_per_cluster as u64
                        * 512,
                    atime: TimeSpec { sec: 0, nsec: 0 },
                    mtime: TimeSpec { sec: 0, nsec: 0 },
                    ctime: TimeSpec { sec: 0, nsec: 0 },
                    uid: 0,
                    gid: 0,
                }),
                vfs: Arc::downgrade(&fatfs),
            };

            icache.submit(info.root_cluster as Ino, Arc::new(root_inode))?
        };

        Ok((fatfs, root_inode))
    }
}

impl Vfs for FatFs {
    fn io_blksize(&self) -> usize {
        4096
    }

    fn fs_devid(&self) -> DevId {
        self.device.devid()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct FatInode {
    idata: Mutex<InodeData>,
    vfs: Weak<Mutex<FatFs>>,
}

struct ClusterIterator<'lt> {
    fat: &'lt [ClusterNo],
    cur: ClusterNo,
}

impl<'lt> ClusterIterator<'lt> {
    fn new(fat: &'lt [ClusterNo], start: ClusterNo) -> Self {
        Self { fat, cur: start }
    }
}

impl<'lt> Iterator for ClusterIterator<'lt> {
    type Item = ClusterNo;

    fn next(&mut self) -> Option<Self::Item> {
        const EOC: ClusterNo = 0x0FFFFFF8;
        let next = self.cur;

        if next >= EOC {
            None
        } else {
            self.cur = self.fat[next as usize];
            Some(next)
        }
    }
}

impl Inode for FatInode {
    fn idata(&self) -> &Mutex<InodeData> {
        &self.idata
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn read(&self, buffer: &mut [u8], offset: usize) -> KResult<usize> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let vfs = vfs.lock();
        let fat = vfs.fat.lock();

        let cluster_size = vfs.sectors_per_cluster as usize * 512;

        let buffer_len = buffer.len();
        let skip_count = offset / cluster_size;
        let inner_offset = offset % cluster_size;
        let cluster_count =
            (inner_offset + buffer.len() + cluster_size - 1) / cluster_size;

        let mut cluster_iter =
            ClusterIterator::new(&fat, self.idata.lock().ino as ClusterNo)
                .skip(skip_count)
                .take(cluster_count);

        let page_buffer = Page::alloc_one();

        let mut nread = 0;
        if let Some(cluster) = cluster_iter.next() {
            vfs.read_cluster(cluster, &page_buffer)?;

            let (_, data) = page_buffer
                .as_cached()
                .as_slice::<u8>(page_buffer.len())
                .split_at(inner_offset);

            if data.len() > buffer_len - nread {
                buffer[nread..].copy_from_slice(&data[..buffer_len - nread]);
                return Ok(buffer_len);
            } else {
                buffer[nread..nread + data.len()].copy_from_slice(data);
                nread += data.len();
            }
        }

        for cluster in cluster_iter {
            vfs.read_cluster(cluster, &page_buffer)?;

            let data =
                page_buffer.as_cached().as_slice::<u8>(page_buffer.len());

            if data.len() > buffer_len - nread {
                buffer[nread..].copy_from_slice(&data[..buffer_len - nread]);
                return Ok(buffer_len);
            } else {
                buffer[nread..nread + data.len()].copy_from_slice(data);
                nread += data.len();
            }
        }

        Ok(nread)
    }

    fn readdir(
        &self,
        offset: usize,
        callback: &mut ReadDirCallback,
    ) -> KResult<usize> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let vfs = vfs.lock();

        let fat = vfs.fat.lock();

        let cluster_size = vfs.sectors_per_cluster as usize * 512;
        let skip_count = offset / cluster_size;
        let inner_offset = offset % cluster_size;

        let cluster_iter =
            ClusterIterator::new(&fat, self.idata.lock().ino as ClusterNo)
                .skip(skip_count)
                .enumerate();

        let mut nread = 0;
        let buffer = Page::alloc_one();
        for (idx, cluster) in cluster_iter {
            vfs.read_cluster(cluster, &buffer)?;

            const ENTRY_SIZE: usize = core::mem::size_of::<FatDirectoryEntry>();
            let count = cluster_size / ENTRY_SIZE;

            let entries = {
                let entries = buffer
                    .as_cached()
                    .as_slice::<FatDirectoryEntry>(count)
                    .iter();

                entries.skip(if idx == 0 {
                    inner_offset / ENTRY_SIZE
                } else {
                    0
                })
            };

            for entry in entries {
                if entry.is_invalid() {
                    nread += ENTRY_SIZE;
                    continue;
                }

                let ino = entry.ino();
                let name = entry.filename()?;

                let inode = {
                    let mut icache = vfs.icache.lock();

                    match icache.get(ino) {
                        Some(inode) => inode,
                        None => {
                            let nlink;
                            let mut mode = 0o777;

                            if entry.is_directory() {
                                nlink = 2;
                                mode |= S_IFDIR;
                            } else {
                                nlink = 1;
                                mode |= S_IFREG;
                            }

                            let inode = Arc::new(FatInode {
                                idata: Mutex::new(InodeData {
                                    ino,
                                    mode,
                                    nlink,
                                    size: entry.size as u64,
                                    atime: TimeSpec::default(),
                                    mtime: TimeSpec::default(),
                                    ctime: TimeSpec::default(),
                                    uid: 0,
                                    gid: 0,
                                }),
                                vfs: self.vfs.clone(),
                            });

                            icache.submit(ino, inode)?
                        }
                    }
                };

                if callback(name.as_str(), &inode, &inode.idata().lock(), 0)
                    .is_err()
                {
                    return Ok(nread);
                }

                nread += ENTRY_SIZE;
            }
        }

        Ok(nread)
    }

    fn vfs_weak(&self) -> Weak<Mutex<dyn Vfs>> {
        self.vfs.clone()
    }

    fn vfs_strong(&self) -> Option<Arc<Mutex<dyn Vfs>>> {
        match self.vfs.upgrade() {
            Some(vfs) => Some(vfs),
            None => None,
        }
    }
}

struct FatMountCreator;

impl MountCreator for FatMountCreator {
    fn create_mount(
        &self,
        _source: &str,
        _flags: u64,
        _data: &[u8],
    ) -> KResult<Mount> {
        let (fatfs, root_inode) = FatFs::create(make_device(8, 1))?;

        Ok(Mount::new(fatfs, root_inode))
    }
}

pub fn init() {
    register_filesystem("fat32", Box::new(FatMountCreator)).unwrap();
}
