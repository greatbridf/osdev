use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use bindings::{EINVAL, EIO, S_IFDIR, S_IFREG};

use crate::{
    io::copy_offset_count,
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

const EOC: ClusterNo = 0x0FFFFFF8;

/// Convert a mutable reference to a slice of bytes
/// This is a safe wrapper around `core::slice::from_raw_parts_mut`
///
fn as_slice<T>(object: &mut [T]) -> &mut [u8] {
    unsafe {
        core::slice::from_raw_parts_mut(
            object.as_mut_ptr() as *mut u8,
            object.len() * core::mem::size_of::<T>(),
        )
    }
}

/// Convert a slice of bytes to a mutable reference
///
fn as_object<T>(slice: &[u8]) -> &T {
    assert_eq!(slice.len(), core::mem::size_of::<T>());
    unsafe { &*(slice.as_ptr() as *const T) }
}

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
    device: BlockDevice,
    icache: Mutex<InodeCache<FatFs>>,
    sectors_per_cluster: u8,
    rootdir_cluster: ClusterNo,
    data_start: u64,
    fat: Mutex<Vec<ClusterNo>>,
    volume_label: String,
}

impl FatFs {
    // /// Read a sector
    // fn read_sector(&self, sector: u64, buf: &mut [u8]) -> KResult<()> {
    //     assert_eq!(buf.len(), 512);
    //     let mut rq = BlockDeviceRequest {
    //         sector,
    //         count: 1,
    //         buffer: Page::alloc_one(),
    //     };
    //     self.read(&mut rq)?;

    //     buf.copy_from_slice(rq.buffer.as_cached().as_slice(512));

    //     Ok(())
    // }

    fn read_cluster(&self, cluster: ClusterNo, buf: &mut [u8]) -> KResult<()> {
        let cluster = cluster - 2;

        let mut rq = BlockDeviceRequest {
            sector: self.data_start as u64
                + cluster as u64 * self.sectors_per_cluster as u64,
            count: self.sectors_per_cluster as u64,
            buffer: buf,
        };
        self.device.read(&mut rq)?;

        Ok(())
    }
}

impl FatFs {
    pub fn create(
        device: DevId,
    ) -> KResult<(Arc<Mutex<Self>>, Arc<dyn Inode>)> {
        let mut fatfs = Self {
            device: BlockDevice::new(device),
            icache: Mutex::new(InodeCache::new()),
            sectors_per_cluster: 0,
            rootdir_cluster: 0,
            data_start: 0,
            fat: Mutex::new(Vec::new()),
            volume_label: String::new(),
        };

        let mut info = [0u8; 512];

        let info = {
            let mut rq = BlockDeviceRequest {
                sector: 0,
                count: 1,
                // buffer: Page::alloc_one(),
                buffer: &mut info,
            };
            fatfs.device.read(&mut rq)?;

            as_object::<Bootsector>(&info)
        };

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

            let mut rq = BlockDeviceRequest {
                sector: info.reserved_sectors as u64,
                count: info.sectors_per_fat as u64,
                buffer: unsafe {
                    core::slice::from_raw_parts_mut(
                        fat.as_mut_ptr() as *mut _,
                        fat.len() * core::mem::size_of::<ClusterNo>(),
                    )
                },
            };
            fatfs.device.read(&mut rq)?;
        }

        fatfs.volume_label = String::from(
            str::from_utf8(&info.volume_label)
                .map_err(|_| EINVAL)?
                .trim_end_matches(char::from(' ')),
        );

        let root_dir_cluster_count = {
            let fat = fatfs.fat.lock();
            let mut next = fatfs.rootdir_cluster;
            let mut count = 1;
            loop {
                next = fat[next as usize];
                if next >= EOC {
                    break;
                }
                count += 1;
            }

            count
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

impl Inode for FatInode {
    fn idata(&self) -> &Mutex<InodeData> {
        &self.idata
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn read(
        &self,
        mut buffer: &mut [u8],
        mut count: usize,
        mut offset: usize,
    ) -> KResult<usize> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let vfs = vfs.lock();
        let fat = vfs.fat.lock();

        let cluster_size = vfs.sectors_per_cluster as usize * 512;
        let mut cno = {
            let idata = self.idata.lock();
            idata.ino as ClusterNo
        };

        while offset >= cluster_size {
            cno = fat[cno as usize];
            offset -= cluster_size;

            if cno >= EOC {
                return Ok(0);
            }
        }

        let page_buffer = Page::alloc_one();
        let page_buffer = page_buffer
            .as_cached()
            .as_mut_slice::<u8>(page_buffer.len());

        let orig_count = count;
        while count != 0 {
            vfs.read_cluster(cno, page_buffer)?;

            let ncopied = copy_offset_count(page_buffer, buffer, offset, count);
            offset = 0;

            if ncopied == 0 {
                break;
            }

            count -= ncopied;
            buffer = &mut buffer[ncopied..];

            cno = fat[cno as usize];
            if cno >= EOC {
                break;
            }
        }

        Ok(orig_count - count)
    }

    fn readdir(
        &self,
        offset: usize,
        callback: &mut ReadDirCallback,
    ) -> KResult<usize> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let vfs = vfs.lock();

        let fat = vfs.fat.lock();

        let idata = self.idata.lock();
        let mut next = idata.ino as ClusterNo;

        let skip = offset / 512 / vfs.sectors_per_cluster as usize;
        let mut offset = offset % (512 * vfs.sectors_per_cluster as usize);
        for _ in 0..skip {
            if next >= EOC {
                return Ok(0);
            }
            next = fat[next as usize];
        }
        if next >= EOC {
            return Ok(0);
        }

        let mut nread = 0;
        let buffer = Page::alloc_one();
        let buffer = buffer.as_cached().as_mut_slice::<FatDirectoryEntry>(
            vfs.sectors_per_cluster as usize * 512
                / core::mem::size_of::<FatDirectoryEntry>(),
        );
        loop {
            vfs.read_cluster(next, as_slice(buffer))?;
            let start = offset / core::mem::size_of::<FatDirectoryEntry>();
            let end = vfs.sectors_per_cluster as usize * 512
                / core::mem::size_of::<FatDirectoryEntry>();
            offset = 0;

            for entry in buffer.iter().skip(start).take(end - start) {
                if entry.attr & ATTR_VOLUME_ID != 0 {
                    nread += core::mem::size_of::<FatDirectoryEntry>();
                    continue;
                }

                let cluster_high = (entry.cluster_high as u32) << 16;
                let ino = (entry.cluster_low as u32 | cluster_high) as Ino;

                let name = {
                    let mut name = String::new();
                    name += str::from_utf8(&entry.name)
                        .map_err(|_| EINVAL)?
                        .trim_end_matches(char::from(' '));

                    if entry.extension[0] != ' ' as u8 {
                        name.push('.');
                    }

                    name += str::from_utf8(&entry.extension)
                        .map_err(|_| EINVAL)?
                        .trim_end_matches(char::from(' '));

                    if entry.reserved & RESERVED_FILENAME_LOWERCASE != 0 {
                        name.make_ascii_lowercase();
                    }
                    name
                };

                let inode = {
                    let mut icache = vfs.icache.lock();

                    match icache.get(ino) {
                        Some(inode) => inode,
                        None => {
                            let is_directory = entry.attr & ATTR_DIRECTORY != 0;
                            let inode = Arc::new(FatInode {
                                idata: Mutex::new(InodeData {
                                    ino,
                                    mode: 0o777
                                        | if is_directory {
                                            S_IFDIR
                                        } else {
                                            S_IFREG
                                        },
                                    nlink: if is_directory { 2 } else { 1 },
                                    size: entry.size as u64,
                                    atime: TimeSpec { sec: 0, nsec: 0 },
                                    mtime: TimeSpec { sec: 0, nsec: 0 },
                                    ctime: TimeSpec { sec: 0, nsec: 0 },
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

                nread += core::mem::size_of::<FatDirectoryEntry>();
            }
            next = fat[next as usize];
            if next >= EOC {
                return Ok(nread);
            }
        }
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
