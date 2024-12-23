use core::{ops::ControlFlow, sync::atomic::Ordering};

use alloc::{
    collections::btree_map::BTreeMap,
    sync::{Arc, Weak},
    vec::Vec,
};
use bindings::{EINVAL, EIO};

use itertools::Itertools;

use crate::{
    io::{Buffer, RawBuffer, UninitBuffer},
    kernel::{
        block::{make_device, BlockDevice, BlockDeviceRequest},
        constants::{S_IFDIR, S_IFREG},
        mem::{paging::Page, phys::PhysPtr},
        vfs::{
            dentry::Dentry,
            inode::{define_struct_inode, Ino, Inode, InodeData},
            mount::{register_filesystem, Mount, MountCreator},
            vfs::Vfs,
            DevId,
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
    pub fn filename(&self) -> Arc<[u8]> {
        let fnpos = self.name.iter().position(|&c| c == ' ' as u8).unwrap_or(8);
        let mut name = self.name[..fnpos].to_vec();

        let extpos = self
            .extension
            .iter()
            .position(|&c| c == ' ' as u8)
            .unwrap_or(3);

        if extpos != 0 {
            name.push('.' as u8);
            name.extend_from_slice(&self.extension[..extpos]);
        }

        if self.reserved & RESERVED_FILENAME_LOWERCASE != 0 {
            name.make_ascii_lowercase();
        }

        name.into()
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

impl_any!(FatFs);
/// # Lock order
/// 2. FatTable
/// 3. Inodes
///
struct FatFs {
    sectors_per_cluster: u8,
    rootdir_cluster: ClusterNo,
    data_start: u64,
    volume_label: [u8; 11],

    device: Arc<BlockDevice>,
    fat: RwSemaphore<Vec<ClusterNo>>,
    weak: Weak<FatFs>,
    icache: BTreeMap<Ino, FatInode>,
}

impl Vfs for FatFs {
    fn io_blksize(&self) -> usize {
        4096
    }

    fn fs_devid(&self) -> DevId {
        self.device.devid()
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

impl FatFs {
    fn read_cluster(&self, cluster: ClusterNo, buf: &Page) -> KResult<()> {
        let cluster = cluster - 2;

        let rq = BlockDeviceRequest {
            sector: self.data_start as u64 + cluster as u64 * self.sectors_per_cluster as u64,
            count: self.sectors_per_cluster as u64,
            buffer: core::slice::from_ref(buf),
        };
        self.device.read_raw(rq)?;

        Ok(())
    }

    fn get_or_alloc_inode(&self, ino: Ino, is_directory: bool, size: u32) -> Arc<dyn Inode> {
        self.icache
            .get(&ino)
            .cloned()
            .map(FatInode::unwrap)
            .unwrap_or_else(|| {
                if is_directory {
                    DirInode::new(ino, self.weak.clone(), size)
                } else {
                    FileInode::new(ino, self.weak.clone(), size)
                }
            })
    }
}

impl FatFs {
    pub fn create(device: DevId) -> KResult<(Arc<Self>, Arc<dyn Inode>)> {
        let device = BlockDevice::get(device)?;
        let mut fatfs_arc = Arc::new_cyclic(|weak: &Weak<FatFs>| Self {
            device,
            sectors_per_cluster: 0,
            rootdir_cluster: 0,
            data_start: 0,
            fat: RwSemaphore::new(Vec::new()),
            weak: weak.clone(),
            icache: BTreeMap::new(),
            volume_label: [0; 11],
        });

        let fatfs = unsafe { Arc::get_mut_unchecked(&mut fatfs_arc) };

        let mut info: UninitBuffer<Bootsector> = UninitBuffer::new();
        fatfs.device.read_some(0, &mut info)?.ok_or(EIO)?;
        let info = info.assume_filled_ref()?;

        fatfs.sectors_per_cluster = info.sectors_per_cluster;
        fatfs.rootdir_cluster = info.root_cluster;
        fatfs.data_start =
            info.reserved_sectors as u64 + info.fat_copies as u64 * info.sectors_per_fat as u64;

        let fat = fatfs.fat.get_mut();

        fat.resize(
            512 * info.sectors_per_fat as usize / core::mem::size_of::<ClusterNo>(),
            0,
        );

        let mut buffer = RawBuffer::new_from_slice(fat.as_mut_slice());

        fatfs
            .device
            .read_some(info.reserved_sectors as usize * 512, &mut buffer)?
            .ok_or(EIO)?;

        if !buffer.filled() {
            return Err(EIO);
        }

        info.volume_label
            .iter()
            .take_while(|&&c| c != ' ' as u8)
            .take(11)
            .enumerate()
            .for_each(|(idx, c)| fatfs.volume_label[idx] = *c);

        let root_dir_cluster_count = ClusterIterator::new(fat, fatfs.rootdir_cluster).count();
        let root_dir_size = root_dir_cluster_count as u32 * info.sectors_per_cluster as u32 * 512;
        let root_inode = DirInode::new(info.root_cluster as Ino, fatfs.weak.clone(), root_dir_size);

        Ok((fatfs_arc, root_inode))
    }
}

struct ClusterIterator<'fat> {
    fat: &'fat [ClusterNo],
    cur: ClusterNo,
}

impl<'fat> ClusterIterator<'fat> {
    fn new(fat: &'fat [ClusterNo], start: ClusterNo) -> Self {
        Self { fat, cur: start }
    }

    fn read<'closure, 'vfs>(
        self,
        vfs: &'vfs FatFs,
        offset: usize,
    ) -> impl Iterator<Item = KResult<&'closure [u8]>> + 'closure
    where
        'fat: 'closure,
        'vfs: 'closure,
    {
        let cluster_size = vfs.sectors_per_cluster as usize * 512;

        let skip_count = offset / cluster_size;
        let mut inner_offset = offset % cluster_size;

        let page_buffer = Page::alloc_one();

        self.skip(skip_count).map(move |cluster| {
            vfs.read_cluster(cluster, &page_buffer)?;

            let data = page_buffer
                .as_cached()
                .as_slice::<u8>(page_buffer.len())
                .split_at(inner_offset)
                .1;
            inner_offset = 0;

            Ok(data)
        })
    }

    fn dirs<'closure, 'vfs>(
        self,
        vfs: &'vfs FatFs,
        offset: usize,
    ) -> impl Iterator<Item = KResult<&'closure FatDirectoryEntry>> + 'closure
    where
        'fat: 'closure,
        'vfs: 'closure,
    {
        const ENTRY_SIZE: usize = core::mem::size_of::<FatDirectoryEntry>();
        self.read(vfs, offset)
            .map(|result| {
                let data = result?;
                if data.len() % ENTRY_SIZE != 0 {
                    return Err(EINVAL);
                }

                Ok(unsafe {
                    core::slice::from_raw_parts(
                        data.as_ptr() as *const FatDirectoryEntry,
                        data.len() / ENTRY_SIZE,
                    )
                })
            })
            .flatten_ok()
    }
}

impl<'fat> Iterator for ClusterIterator<'fat> {
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

#[derive(Clone)]
enum FatInode {
    File(Arc<FileInode>),
    Dir(Arc<DirInode>),
}

impl FatInode {
    fn unwrap(self) -> Arc<dyn Inode> {
        match self {
            FatInode::File(inode) => inode,
            FatInode::Dir(inode) => inode,
        }
    }
}

define_struct_inode! {
    struct FileInode;
}

impl FileInode {
    fn new(ino: Ino, weak: Weak<FatFs>, size: u32) -> Arc<Self> {
        let inode = Arc::new(Self {
            idata: InodeData::new(ino, weak),
        });

        // Safety: We are initializing the inode
        inode.nlink.store(1, Ordering::Relaxed);
        inode.mode.store(S_IFREG | 0o777, Ordering::Relaxed);
        inode.size.store(size as u64, Ordering::Relaxed);

        inode
    }
}

impl Inode for FileInode {
    fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let vfs = vfs.as_any().downcast_ref::<FatFs>().unwrap();
        let fat = vfs.fat.lock_shared();

        let iter = ClusterIterator::new(fat.as_ref(), self.ino as ClusterNo).read(vfs, offset);

        for data in iter {
            if buffer.fill(data?)?.should_stop() {
                break;
            }
        }

        Ok(buffer.wrote())
    }
}

define_struct_inode! {
    struct DirInode;
}

impl DirInode {
    fn new(ino: Ino, weak: Weak<FatFs>, size: u32) -> Arc<Self> {
        let inode = Arc::new(Self {
            idata: InodeData::new(ino, weak),
        });

        // Safety: We are initializing the inode
        inode.nlink.store(2, Ordering::Relaxed);
        inode.mode.store(S_IFDIR | 0o777, Ordering::Relaxed);
        inode.size.store(size as u64, Ordering::Relaxed);

        inode
    }
}

impl Inode for DirInode {
    fn lookup(&self, dentry: &Arc<Dentry>) -> KResult<Option<Arc<dyn Inode>>> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let vfs = vfs.as_any().downcast_ref::<FatFs>().unwrap();
        let fat = vfs.fat.lock_shared();

        let mut entries = ClusterIterator::new(fat.as_ref(), self.ino as ClusterNo).dirs(vfs, 0);

        let entry = entries.find_map(|entry| {
            if entry.is_err() {
                return Some(entry);
            }

            let entry = entry.unwrap();

            if !entry.is_invalid() && entry.filename().eq(dentry.name()) {
                Some(Ok(entry))
            } else {
                None
            }
        });

        match entry {
            None => Ok(None),
            Some(Err(err)) => Err(err),
            Some(Ok(entry)) => {
                let ino = entry.ino();

                Ok(Some(vfs.get_or_alloc_inode(
                    ino,
                    entry.is_directory(),
                    entry.size,
                )))
            }
        }
    }

    fn do_readdir(
        &self,
        offset: usize,
        callback: &mut dyn FnMut(&[u8], Ino) -> KResult<ControlFlow<(), ()>>,
    ) -> KResult<usize> {
        let vfs = self.vfs.upgrade().ok_or(EIO)?;
        let vfs = vfs.as_any().downcast_ref::<FatFs>().unwrap();
        let fat = vfs.fat.lock_shared();

        const ENTRY_SIZE: usize = core::mem::size_of::<FatDirectoryEntry>();
        let cluster_iter =
            ClusterIterator::new(fat.as_ref(), self.ino as ClusterNo).dirs(vfs, offset);

        let mut nread = 0;
        for entry in cluster_iter {
            let entry = entry?;

            if entry.is_invalid() {
                nread += 1;
                continue;
            }

            let ino = entry.ino();
            let name = entry.filename();

            vfs.get_or_alloc_inode(ino, entry.is_directory(), entry.size);

            if callback(name.as_ref(), ino)?.is_break() {
                break;
            }

            nread += 1;
        }

        Ok(nread * ENTRY_SIZE)
    }
}

struct FatMountCreator;

impl MountCreator for FatMountCreator {
    fn create_mount(&self, _source: &str, _flags: u64, mp: &Arc<Dentry>) -> KResult<Mount> {
        let (fatfs, root_inode) = FatFs::create(make_device(8, 1))?;

        Mount::new(mp, fatfs, root_inode)
    }
}

pub fn init() {
    register_filesystem("fat32", Arc::new(FatMountCreator)).unwrap();
}
