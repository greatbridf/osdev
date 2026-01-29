mod mbr;

use alloc::collections::btree_map::{BTreeMap, Entry};
use alloc::sync::Arc;
use core::cmp::Ordering;

use async_trait::async_trait;
use mbr::MBRPartTable;

use super::constants::ENOENT;
use super::mem::Folio;
use super::vfs::types::DeviceId;
use crate::io::{Buffer, Chunks, FillResult};
use crate::kernel::constants::{EEXIST, EINVAL};
use crate::prelude::*;

pub struct Partition {
    pub lba_offset: u64,
    pub sector_count: u64,
}

pub trait PartTable {
    fn partitions(&self) -> impl Iterator<Item = Partition> + use<'_, Self>;
}

#[async_trait]
pub trait BlockRequestQueue: Send + Sync {
    /// Maximum number of sectors that can be read in one request
    fn max_request_pages(&self) -> u64;

    async fn submit<'a>(&'a self, req: BlockDeviceRequest<'a>) -> KResult<()>;
}

enum BlockDeviceType {
    Disk {
        queue: Arc<dyn BlockRequestQueue>,
    },
    Partition {
        disk_dev: DeviceId,
        lba_offset: u64,
        queue: Arc<dyn BlockRequestQueue>,
    },
}

pub struct BlockDevice {
    /// Unique device identifier, major and minor numbers
    devid: DeviceId,
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

static BLOCK_DEVICE_LIST: Spin<BTreeMap<DeviceId, Arc<BlockDevice>>> = Spin::new(BTreeMap::new());

impl BlockDevice {
    pub fn register_disk(
        devid: DeviceId,
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

    pub fn get(devid: DeviceId) -> KResult<Arc<Self>> {
        BLOCK_DEVICE_LIST.lock().get(&devid).cloned().ok_or(ENOENT)
    }
}

impl BlockDevice {
    pub fn devid(&self) -> DeviceId {
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
            devid: DeviceId::new(self.devid.major, self.devid.minor + idx as u16 + 1),
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
    pub async fn commit_request(&self, mut req: BlockDeviceRequest<'_>) -> KResult<()> {
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

        self.queue().submit(req).await
    }

    /// Read some from the block device, may involve some copy and fragmentation
    ///
    /// Further optimization may be needed, including caching, read-ahead and reordering
    ///
    /// # Arguments
    /// `offset` - offset in bytes
    ///
    pub async fn read_some(&self, offset: usize, buffer: &mut dyn Buffer) -> KResult<FillResult> {
        let sector_start = offset as u64 / 512;
        let mut first_sector_offset = offset % 512;
        let nr_sectors = (first_sector_offset + buffer.total() + 511) / 512;

        let nr_sectors_per_batch = self.queue().max_request_pages() / 2 * 2 * 8;

        let mut nr_filled = 0;
        for (start, nr_batch) in Chunks::new(sector_start, nr_sectors as u64, nr_sectors_per_batch)
        {
            let (page_slice, page, mut page_vec);
            match nr_batch {
                ..=8 => {
                    page = Folio::alloc();
                    page_slice = core::slice::from_ref(&page);
                }
                ..=16 => {
                    page = Folio::alloc_order(1);
                    page_slice = core::slice::from_ref(&page);
                }
                ..=32 => {
                    page = Folio::alloc_order(2);
                    page_slice = core::slice::from_ref(&page);
                }
                count => {
                    let nr_huge_pages = count as usize / 32;
                    let nr_small_pages = ((count as usize % 32) + 7) / 8;

                    let nr_pages = nr_huge_pages + nr_small_pages;
                    page_vec = Vec::with_capacity(nr_pages);

                    page_vec.resize_with(nr_huge_pages, || Folio::alloc_order(2));
                    page_vec.resize_with(nr_pages, || Folio::alloc());
                    page_slice = &page_vec;
                }
            }

            let req = BlockDeviceRequest::Read {
                sector: start,
                count: nr_batch,
                buffer: page_slice,
            };

            self.commit_request(req).await?;

            for page in page_slice {
                let pg = page.lock();
                let data = &pg.as_bytes()[first_sector_offset..];
                first_sector_offset = 0;

                nr_filled += buffer.fill(data)?.allow_partial();

                if buffer.available() == 0 {
                    break;
                }
            }

            if buffer.available() == 0 {
                break;
            }
        }

        if buffer.available() == 0 {
            Ok(FillResult::Done(nr_filled))
        } else {
            Ok(FillResult::Partial(nr_filled))
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
        buffer: &'lt [Folio],
    },
    Write {
        /// Sector to write to, in 512-byte blocks
        sector: u64,
        /// Number of sectors to write
        count: u64,
        /// Buffer pages to write from
        buffer: &'lt [Folio],
    },
}
