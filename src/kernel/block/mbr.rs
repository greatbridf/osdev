use super::{BlockDevice, PartTable, Partition};
use crate::{
    io::UninitBuffer,
    kernel::constants::{EIO, ENODEV},
    prelude::KResult,
};

#[repr(C)]
#[derive(Clone, Copy)]
struct MBREntry {
    attr: u8,
    chs_start: [u8; 3],
    part_type: u8,
    chs_end: [u8; 3],
    lba_start: u32,
    cnt: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct MBRData {
    code: [u8; 446],
    entries: [MBREntry; 4],
    magic: [u8; 2],
}

pub struct MBRPartTable {
    entries: [MBREntry; 4],
}

impl MBRPartTable {
    pub async fn from_disk(disk: &BlockDevice) -> KResult<Self> {
        let mut mbr: UninitBuffer<MBRData> = UninitBuffer::new();
        disk.read_some(0, &mut mbr)?.ok_or(EIO)?;
        let mbr = mbr.assume_init()?;

        if mbr.magic != [0x55, 0xaa] {
            Err(ENODEV)?;
        }

        Ok(Self {
            entries: mbr.entries,
        })
    }
}

impl PartTable for MBRPartTable {
    fn partitions(&self) -> impl Iterator<Item = Partition> + use<'_> {
        self.entries
            .iter()
            .filter(|entry| entry.part_type != 0)
            .map(|entry| Partition {
                lba_offset: entry.lba_start as u64,
                sector_count: entry.cnt as u64,
            })
    }
}
