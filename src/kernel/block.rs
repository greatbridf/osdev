mod mbr;

use super::{
    constants::ENOENT,
    mem::{paging::Page, AsMemoryBlock as _},
    vfs::DevId,
};
use crate::{
    io::ByteBuffer,
    kernel::constants::{EEXIST, EINVAL, EIO}
};
use crate::{
    io::{Buffer, FillResult},
    prelude::*,
};
use alloc::{
    collections::btree_map::{BTreeMap, Entry},
    sync::Arc,
};
use core::cmp::Ordering;
use mbr::MBRPartTable;

pub fn make_device(major: u32, minor: u32) -> DevId {
    (major << 8) & 0xff00u32 | minor & 0xffu32
}

pub struct Partition {
    pub lba_offset: u64,
    pub sector_count: u64,
}

pub trait PartTable {
    fn partitions(&self) -> impl Iterator<Item = Partition> + use<'_, Self>;
}

pub trait BlockRequestQueue: Send + Sync {
    /// Maximum number of sectors that can be read in one request
    fn max_request_pages(&self) -> u64;

    fn submit(&self, req: BlockDeviceRequest) -> KResult<()>;
}

enum BlockDeviceType {
    Disk {
        queue: Arc<dyn BlockRequestQueue>,
    },
    Partition {
        disk_dev: DevId,
        lba_offset: u64,
        queue: Arc<dyn BlockRequestQueue>,
    },
}

#[derive(Debug, Clone)]
pub enum FileSystemType {
    Ext4,
    Fat32,
}

pub struct BlockDevice {
    /// Unique device identifier, major and minor numbers
    devid: DevId,
    /// Total size of the device in sectors (512 bytes each)
    sector_count: u64,

    dev_type: BlockDeviceType,
}

impl PartialEq for BlockDevice {
    fn eq(&self, other: &Self) -> bool {
        self.devid == other.devid
    }
}

impl PartialOrd for BlockDevice {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.devid.cmp(&other.devid))
    }
}

impl Eq for BlockDevice {}

impl Ord for BlockDevice {
    fn cmp(&self, other: &Self) -> Ordering {
        self.devid.cmp(&other.devid)
    }
}

static BLOCK_DEVICE_LIST: Spin<BTreeMap<DevId, Arc<BlockDevice>>> = Spin::new(BTreeMap::new());

impl BlockDevice {
    pub fn register_disk(
        devid: DevId,
        size: u64,
        queue: Arc<dyn BlockRequestQueue>,
    ) -> KResult<Arc<Self>> {
        let device = Arc::new(Self {
            devid,
            sector_count: size,
            dev_type: BlockDeviceType::Disk { queue },
        });

        match BLOCK_DEVICE_LIST.lock().entry(devid) {
            Entry::Vacant(entry) => Ok(entry.insert(device).clone()),
            Entry::Occupied(_) => Err(EEXIST),
        }
    }

    pub fn get(devid: DevId) -> KResult<Arc<Self>> {
        BLOCK_DEVICE_LIST.lock().get(&devid).cloned().ok_or(ENOENT)
    }
}

impl BlockDevice {
    pub fn devid(&self) -> DevId {
        self.devid
    }

    fn queue(&self) -> &Arc<dyn BlockRequestQueue> {
        match &self.dev_type {
            BlockDeviceType::Disk { queue } => queue,
            BlockDeviceType::Partition { queue, .. } => queue,
        }
    }

    pub fn register_partition(&self, idx: u32, offset: u64, size: u64) -> KResult<Arc<Self>> {
        let queue = match &self.dev_type {
            BlockDeviceType::Disk { queue } => queue.clone(),
            BlockDeviceType::Partition { .. } => return Err(EINVAL),
        };

        let device = Arc::new(BlockDevice {
            devid: make_device(self.devid >> 8, idx as u32),
            sector_count: size,
            dev_type: BlockDeviceType::Partition {
                disk_dev: self.devid,
                lba_offset: offset,
                queue,
            },
        });

        match BLOCK_DEVICE_LIST.lock().entry(device.devid()) {
            Entry::Vacant(entry) => Ok(entry.insert(device).clone()),
            Entry::Occupied(_) => Err(EEXIST),
        }
    }

    fn is_ext4_signature(&self, buffer: &[u8]) -> bool {
        if buffer.len() >= 1080 + 2 {
            &buffer[1080..1082] == &[0x53, 0xEF]
        } else {
            false
        }
    }

    fn is_fat32_signature(&self, buffer: &[u8]) -> bool {
        if buffer.len() >= 90 {
            &buffer[82..90] == b"FAT32   "
        } else {
            false
        }
    }

    fn detect_filesystem(&self) -> KResult<FileSystemType> {
        let mut buffer = vec![0u8; 4096];
        let mut byte_buffer = ByteBuffer::new(buffer.as_mut_slice());
        self.read_some(0, &mut byte_buffer)?.ok_or(EIO)?;

        if self.is_ext4_signature(&buffer) {
            Ok(FileSystemType::Ext4)
        } else if self.is_fat32_signature(&buffer) {
            Ok(FileSystemType::Fat32)
        } else {
            Err(0)
        }
    }

    fn register_whole_disk_partition(&self) -> KResult<Arc<Self>> {
        let queue = match &self.dev_type {
            BlockDeviceType::Disk { queue } => queue.clone(),
            BlockDeviceType::Partition { .. } => return Err(EINVAL),
        };

        let device = Arc::new(BlockDevice {
            devid: make_device(self.devid >> 8, 0u32),
            sector_count: self.sector_count,
            dev_type: BlockDeviceType::Partition {
                disk_dev: self.devid,
                lba_offset: 0,
                queue,
            },
        });

        match BLOCK_DEVICE_LIST.lock().entry(device.devid()) {
            Entry::Vacant(entry) => Ok(entry.insert(device).clone()),
            Entry::Occupied(_) => Err(EEXIST),
        }
    }

    pub async fn partprobe(&self) -> KResult<()> {
        match self.dev_type {
            BlockDeviceType::Partition { .. } => Err(EINVAL),
            BlockDeviceType::Disk { .. } => {
                let base_minor = (self.devid & 0xFF) as u32;
                if let Ok(mbr_table) = MBRPartTable::from_disk(self).await {
                    for (idx, partition) in mbr_table.partitions().enumerate() {
                        self.register_partition(
                            base_minor + idx as u32 + 1,
                            partition.lba_offset,
                            partition.sector_count,
                        )?;
                    }
                    Ok(())
                } else {
                    match self.detect_filesystem() {
                        Ok(_fs_type) => {
                            self.register_whole_disk_partition();
                            Ok(())
                        }
                        Err(_) => Ok(()),
                    }
                }
            }
        }
    }

    /// No extra overhead, send the request directly to the queue
    /// If any of the parameters does not meet the requirement, the operation will fail
    ///
    /// # Requirements
    /// - `req.count` must not exceed the disk size and maximum request size
    /// - `req.sector` must be within the disk size
    /// - `req.buffer` must be enough to hold the data
    ///
    pub fn commit_request(&self, mut req: BlockDeviceRequest) -> KResult<()> {
        // Verify the request parameters.
        match &mut req {
            BlockDeviceRequest::Read { sector, count, .. } => {
                if *sector + *count > self.sector_count {
                    return Err(EINVAL);
                }

                if let BlockDeviceType::Partition { lba_offset, .. } = &self.dev_type {
                    // Adjust the sector for partition offset.
                    *sector += lba_offset;
                }
            }
            BlockDeviceRequest::Write { sector, count, .. } => {
                if *sector + *count > self.sector_count {
                    return Err(EINVAL);
                }

                if let BlockDeviceType::Partition { lba_offset, .. } = &self.dev_type {
                    // Adjust the sector for partition offset.
                    *sector += lba_offset;
                }
            }
        }

        self.queue().submit(req)
    }

    /// Read some from the block device, may involve some copy and fragmentation
    ///
    /// Further optimization may be needed, including caching, read-ahead and reordering
    ///
    /// # Arguments
    /// `offset` - offset in bytes
    ///
    pub fn read_some(&self, offset: usize, buffer: &mut dyn Buffer) -> KResult<FillResult> {
        let mut sector_start = offset as u64 / 512;
        let mut first_sector_offset = offset as u64 % 512;
        let mut sector_count = (first_sector_offset + buffer.total() as u64 + 511) / 512;

        let mut nfilled = 0;
        'outer: while sector_count != 0 {
            let pages: &[Page];
            let page: Option<Page>;
            let page_vec: Option<Vec<Page>>;

            let nread;

            match sector_count {
                count if count <= 8 => {
                    nread = count;

                    let _page = Page::alloc();
                    page = Some(_page);
                    pages = core::slice::from_ref(page.as_ref().unwrap());
                }
                count if count <= 16 => {
                    nread = count;

                    let _pages = Page::alloc_order(1);
                    page = Some(_pages);
                    pages = core::slice::from_ref(page.as_ref().unwrap());
                }
                count => {
                    nread = count.min(self.queue().max_request_pages());

                    let npages = (nread + 15) / 16;
                    let mut _page_vec = Vec::with_capacity(npages as usize);
                    for _ in 0..npages {
                        _page_vec.push(Page::alloc_order(1));
                    }
                    page_vec = Some(_page_vec);
                    pages = page_vec.as_ref().unwrap().as_slice();
                }
            }

            let req = BlockDeviceRequest::Read {
                sector: sector_start,
                count: nread,
                buffer: &pages,
            };

            self.commit_request(req)?;

            for page in pages.iter() {
                // SAFETY: We are the only owner of the page so no one could be mutating it.
                let data = unsafe { &page.as_memblk().as_bytes()[first_sector_offset as usize..] };
                first_sector_offset = 0;

                match buffer.fill(data)? {
                    FillResult::Done(n) => nfilled += n,
                    FillResult::Partial(n) => {
                        nfilled += n;
                        break 'outer;
                    }
                    FillResult::Full => {
                        break 'outer;
                    }
                }
            }

            sector_start += nread;
            sector_count -= nread;
        }

        if nfilled == buffer.total() {
            Ok(FillResult::Done(nfilled))
        } else {
            Ok(FillResult::Partial(nfilled))
        }
    }
}

pub enum BlockDeviceRequest<'lt> {
    Read {
        /// Sector to read from, in 512-byte blocks
        sector: u64,
        /// Number of sectors to read
        count: u64,
        /// Buffer pages to read into
        buffer: &'lt [Page],
    },
    Write {
        /// Sector to write to, in 512-byte blocks
        sector: u64,
        /// Number of sectors to write
        count: u64,
        /// Buffer pages to write from
        buffer: &'lt [Page],
    },
}
