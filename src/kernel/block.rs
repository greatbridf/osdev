use super::{constants::ENOENT, mem::paging::Page, vfs::DevId};
use crate::{
    io::{Buffer, FillResult, UninitBuffer},
    prelude::*,
};
use alloc::{
    collections::btree_map::{BTreeMap, Entry},
    sync::Arc,
};
use bindings::{EEXIST, EINVAL, EIO};
use core::cmp::Ordering;

pub fn make_device(major: u32, minor: u32) -> DevId {
    (major << 8) & 0xff00u32 | minor & 0xffu32
}

pub trait BlockRequestQueue: Send + Sync {
    /// Maximum number of sectors that can be read in one request
    ///
    fn max_request_pages(&self) -> u64;

    fn submit(&self, req: BlockDeviceRequest) -> KResult<()>;
}

struct BlockDeviceDisk {
    queue: Arc<dyn BlockRequestQueue>,
}

#[allow(dead_code)]
struct BlockDevicePartition {
    disk_dev: DevId,
    offset: u64,

    queue: Arc<dyn BlockRequestQueue>,
}

enum BlockDeviceType {
    Disk(BlockDeviceDisk),
    Partition(BlockDevicePartition),
}

pub struct BlockDevice {
    devid: DevId,
    size: u64,
    max_pages: u64,

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

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct MBREntry {
    attr: u8,
    chs_start: [u8; 3],
    part_type: u8,
    chs_end: [u8; 3],
    lba_start: u32,
    cnt: u32,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
struct MBR {
    code: [u8; 446],
    entries: [MBREntry; 4],
    magic: [u8; 2],
}

impl BlockDevice {
    pub fn register_disk(
        devid: DevId,
        size: u64,
        queue: Arc<dyn BlockRequestQueue>,
    ) -> KResult<Arc<Self>> {
        let max_pages = queue.max_request_pages();
        let device = Arc::new(Self {
            devid,
            size,
            max_pages,
            dev_type: BlockDeviceType::Disk(BlockDeviceDisk { queue }),
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

    pub fn register_partition(&self, idx: u32, offset: u64, size: u64) -> KResult<Arc<Self>> {
        let queue = match self.dev_type {
            BlockDeviceType::Disk(ref disk) => disk.queue.clone(),
            BlockDeviceType::Partition(_) => return Err(EINVAL),
        };

        let device = Arc::new(BlockDevice {
            devid: make_device(self.devid >> 8, idx as u32),
            size,
            max_pages: self.max_pages,
            dev_type: BlockDeviceType::Partition(BlockDevicePartition {
                disk_dev: self.devid,
                offset,
                queue,
            }),
        });

        match BLOCK_DEVICE_LIST.lock().entry(device.devid()) {
            Entry::Vacant(entry) => Ok(entry.insert(device).clone()),
            Entry::Occupied(_) => Err(EEXIST),
        }
    }

    pub fn partprobe(&self) -> KResult<()> {
        match self.dev_type {
            BlockDeviceType::Partition(_) => Err(EINVAL),
            BlockDeviceType::Disk(_) => {
                let mut mbr: UninitBuffer<MBR> = UninitBuffer::new();
                self.read_some(0, &mut mbr)?.ok_or(EIO)?;
                let mbr = mbr.assume_filled_ref()?;

                if mbr.magic != [0x55, 0xaa] {
                    return Ok(());
                }

                let entries = mbr.entries;

                for (idx, entry) in entries.iter().enumerate() {
                    if entry.part_type == 0 {
                        continue;
                    }

                    let offset = entry.lba_start as u64;
                    let size = entry.cnt as u64;

                    self.register_partition(idx as u32 + 1, offset, size)?;
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
    pub fn read_raw(&self, mut req: BlockDeviceRequest) -> KResult<()> {
        // TODO: check disk size limit
        if req.sector + req.count > self.size {
            return Err(EINVAL);
        }

        match self.dev_type {
            BlockDeviceType::Disk(ref disk) => disk.queue.submit(req),
            BlockDeviceType::Partition(ref part) => {
                req.sector += part.offset;
                part.queue.submit(req)
            }
        }
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

                    let _page = Page::alloc_one();
                    page = Some(_page);
                    pages = core::slice::from_ref(page.as_ref().unwrap());
                }
                count if count <= 16 => {
                    nread = count;

                    let _pages = Page::alloc_many(1);
                    page = Some(_pages);
                    pages = core::slice::from_ref(page.as_ref().unwrap());
                }
                count => {
                    nread = count.min(self.max_pages);

                    let npages = (nread + 15) / 16;
                    let mut _page_vec = Vec::with_capacity(npages as usize);
                    for _ in 0..npages {
                        _page_vec.push(Page::alloc_many(1));
                    }
                    page_vec = Some(_page_vec);
                    pages = page_vec.as_ref().unwrap().as_slice();
                }
            }

            let req = BlockDeviceRequest {
                sector: sector_start,
                count: nread,
                buffer: &pages,
            };

            self.read_raw(req)?;

            for page in pages.iter() {
                let data = &page.as_slice()[first_sector_offset as usize..];
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

pub struct BlockDeviceRequest<'lt> {
    pub sector: u64, // Sector to read from, in 512-byte blocks
    pub count: u64,  // Number of sectors to read
    pub buffer: &'lt [Page],
}
