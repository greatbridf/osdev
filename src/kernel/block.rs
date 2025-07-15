mod mbr;

use super::{
    constants::ENOENT,
    mem::{paging::Page, AsMemoryBlock as _},
    vfs::DevId,
};
use crate::kernel::constants::{EEXIST, EINVAL};
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

#[derive(Clone)]
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

impl FileSystemType {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileSystemType::Ext4 => "ext4",
            FileSystemType::Fat32 => "fat32",
        }
    }
}

#[derive(Clone)]
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

    pub fn register_partition(&self, idx: usize, offset: u64, size: u64) -> KResult<Arc<Self>> {
        let queue = match &self.dev_type {
            BlockDeviceType::Disk { queue } => queue.clone(),
            BlockDeviceType::Partition { .. } => return Err(EINVAL),
        };

        let device = Arc::new(BlockDevice {
            devid: make_device(self.devid >> 8, (self.devid & 0xff) + idx as u32 + 1),
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

    pub async fn partprobe(&self) -> KResult<()> {
        match self.dev_type {
            BlockDeviceType::Partition { .. } => Err(EINVAL),
            BlockDeviceType::Disk { .. } => {
                if let Ok(mbr_table) = MBRPartTable::from_disk(self).await {
                    for (idx, partition) in mbr_table.partitions().enumerate() {
                        self.register_partition(idx, partition.lba_offset, partition.sector_count)?;
                    }
                }

                Ok(())
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

    /// Write some data to the block device, may involve some copy and fragmentation
    ///
    /// # Arguments
    /// `offset` - offset in bytes
    /// `data` - data to write
    ///
    pub fn write_some(&self, offset: usize, data: &[u8]) -> KResult<usize> {
        let mut sector_start = offset as u64 / 512;
        let mut first_sector_offset = offset as u64 % 512;
        let mut remaining_data = data;
        let mut nwritten = 0;

        while !remaining_data.is_empty() {
            let pages: &[Page];
            let page: Option<Page>;
            let page_vec: Option<Vec<Page>>;

            // Calculate sectors needed for this write
            let write_end = first_sector_offset + remaining_data.len() as u64;
            let sector_count = ((write_end + 511) / 512).min(self.queue().max_request_pages());

            match sector_count {
                count if count <= 8 => {
                    let _page = Page::alloc();
                    page = Some(_page);
                    pages = core::slice::from_ref(page.as_ref().unwrap());
                }
                count if count <= 16 => {
                    let _pages = Page::alloc_order(1);
                    page = Some(_pages);
                    pages = core::slice::from_ref(page.as_ref().unwrap());
                }
                count => {
                    let npages = (count + 15) / 16;
                    let mut _page_vec = Vec::with_capacity(npages as usize);
                    for _ in 0..npages {
                        _page_vec.push(Page::alloc_order(1));
                    }
                    page_vec = Some(_page_vec);
                    pages = page_vec.as_ref().unwrap().as_slice();
                }
            }

            if first_sector_offset != 0 || remaining_data.len() < (sector_count * 512) as usize {
                let read_req = BlockDeviceRequest::Read {
                    sector: sector_start,
                    count: sector_count,
                    buffer: pages,
                };
                self.commit_request(read_req)?;
            }

            let mut data_offset = 0;
            let mut page_offset = first_sector_offset as usize;

            for page in pages.iter() {
                // SAFETY: We own the page and can modify it
                let page_data = unsafe {
                    let memblk = page.as_memblk();
                    core::slice::from_raw_parts_mut(memblk.addr().get() as *mut u8, memblk.len())
                };

                let copy_len =
                    (remaining_data.len() - data_offset).min(page_data.len() - page_offset);

                if copy_len == 0 {
                    break;
                }

                page_data[page_offset..page_offset + copy_len]
                    .copy_from_slice(&remaining_data[data_offset..data_offset + copy_len]);

                data_offset += copy_len;
                page_offset = 0; // Only first page has offset

                if data_offset >= remaining_data.len() {
                    break;
                }
            }

            let write_req = BlockDeviceRequest::Write {
                sector: sector_start,
                count: sector_count,
                buffer: pages,
            };
            self.commit_request(write_req)?;

            let bytes_written = data_offset;
            nwritten += bytes_written;
            remaining_data = &remaining_data[bytes_written..];
            sector_start += sector_count;
            first_sector_offset = 0;
        }

        Ok(nwritten)
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
