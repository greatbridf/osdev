use super::file::ClusterReadIterator;
use crate::kernel::constants::EINVAL;
use crate::prelude::*;
use alloc::{string::String, sync::Arc};
use itertools::Itertools;

#[repr(C, packed)]
pub(super) struct RawDirEntry {
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

pub(super) struct FatDirectoryEntry {
    pub filename: Arc<[u8]>,
    pub cluster: u32,
    pub size: u32,
    pub entry_offset: u32,
    pub is_directory: bool,
    // TODO:
    // create_time: u32,
    // modify_time: u32,
}

impl RawDirEntry {
    const ATTR_RO: u8 = 0x01;
    const ATTR_HIDDEN: u8 = 0x02;
    const ATTR_SYSTEM: u8 = 0x04;
    const ATTR_VOLUME_ID: u8 = 0x08;
    const ATTR_DIRECTORY: u8 = 0x10;
    #[allow(dead_code)]
    const ATTR_ARCHIVE: u8 = 0x20;

    const RESERVED_FILENAME_LOWERCASE: u8 = 0x08;

    fn filename(&self) -> &[u8] {
        self.name.trim_ascii_end()
    }

    fn extension(&self) -> &[u8] {
        self.extension.trim_ascii_end()
    }

    fn is_filename_lowercase(&self) -> bool {
        self.reserved & Self::RESERVED_FILENAME_LOWERCASE != 0
    }

    fn is_long_filename(&self) -> bool {
        self.attr == (Self::ATTR_RO | Self::ATTR_HIDDEN | Self::ATTR_SYSTEM | Self::ATTR_VOLUME_ID)
    }

    fn is_volume_id(&self) -> bool {
        self.attr & Self::ATTR_VOLUME_ID != 0
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
        self.attr & Self::ATTR_DIRECTORY != 0
    }

    fn long_filename(&self) -> Option<[u16; 13]> {
        if !self.is_long_filename() {
            return None;
        }

        let mut name = [0; 13];
        name[0] = u16::from_le_bytes([self.name[1], self.name[2]]);
        name[1] = u16::from_le_bytes([self.name[3], self.name[4]]);
        name[2] = u16::from_le_bytes([self.name[5], self.name[6]]);
        name[3] = u16::from_le_bytes([self.name[7], self.extension[0]]);
        name[4] = u16::from_le_bytes([self.extension[1], self.extension[2]]);
        name[5] = self.create_time;
        name[6] = self.create_date;
        name[7] = self.access_date;
        name[8] = self.cluster_high;
        name[9] = self.modify_time;
        name[10] = self.modify_date;
        name[11] = self.size as u16;
        name[12] = (self.size >> 16) as u16;

        Some(name)
    }
}

impl<'data, I> RawDirs<'data> for I where I: ClusterReadIterator<'data> {}
trait RawDirs<'data>: ClusterReadIterator<'data> {
    fn raw_dirs(self) -> impl Iterator<Item = KResult<&'data RawDirEntry>> + 'data
    where
        Self: Sized,
    {
        const ENTRY_SIZE: usize = size_of::<RawDirEntry>();

        self.map(|result| {
            let data = result?;
            if data.len() % ENTRY_SIZE != 0 {
                return Err(EINVAL);
            }

            Ok(unsafe {
                core::slice::from_raw_parts(
                    data.as_ptr() as *const RawDirEntry,
                    data.len() / ENTRY_SIZE,
                )
            })
        })
        .flatten_ok()
    }
}

pub(super) trait Dirs<'data>: ClusterReadIterator<'data> {
    fn dirs(self) -> impl Iterator<Item = KResult<FatDirectoryEntry>> + 'data
    where
        Self: Sized;
}

impl<'data, I> Dirs<'data> for I
where
    I: ClusterReadIterator<'data>,
{
    fn dirs(self) -> impl Iterator<Item = KResult<FatDirectoryEntry>> + 'data
    where
        Self: Sized,
    {
        self.raw_dirs().real_dirs()
    }
}

trait RealDirs<'data>: Iterator<Item = KResult<&'data RawDirEntry>> + 'data {
    fn real_dirs(self) -> DirsIter<'data, Self>
    where
        Self: Sized;
}

impl<'data, I> RealDirs<'data> for I
where
    I: Iterator<Item = KResult<&'data RawDirEntry>> + 'data,
{
    fn real_dirs(self) -> DirsIter<'data, Self>
    where
        Self: Sized,
    {
        DirsIter { iter: self }
    }
}

pub(super) struct DirsIter<'data, I>
where
    I: Iterator<Item = KResult<&'data RawDirEntry>> + 'data,
{
    iter: I,
}

impl<'data, I> Iterator for DirsIter<'data, I>
where
    I: Iterator<Item = KResult<&'data RawDirEntry>> + 'data,
{
    type Item = KResult<FatDirectoryEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut filename = String::new();
        let mut entry_offset = 0;
        let entry = loop {
            let entry = match self.iter.next()? {
                Ok(entry) => entry,
                Err(err) => return Some(Err(err)),
            };
            entry_offset += 1;

            let long_filename = entry.long_filename();
            if entry.is_invalid() {
                if let Some(long_filename) = long_filename {
                    let long_filename = long_filename
                        .iter()
                        .position(|&ch| ch == 0)
                        .map(|pos| &long_filename[..pos])
                        .unwrap_or(&long_filename);

                    filename.extend(
                        long_filename
                            .into_iter()
                            .map(|&ch| char::from_u32(ch as u32).unwrap_or('?'))
                            .rev(),
                    );
                }
                continue;
            }
            break entry;
        };

        let filename: Arc<[u8]> = if filename.is_empty() {
            let mut filename = entry.filename().to_vec();
            let extension = entry.extension();
            if !extension.is_empty() {
                filename.push(b'.');
                filename.extend_from_slice(extension);
            }

            if entry.is_filename_lowercase() {
                filename.make_ascii_lowercase();
            }

            filename.into()
        } else {
            let mut bytes = filename.into_bytes();
            bytes.reverse();

            bytes.into()
        };

        Some(Ok(FatDirectoryEntry {
            size: entry.size,
            entry_offset,
            filename,
            cluster: entry.cluster_low as u32 | ((entry.cluster_high as u32) << 16),
            is_directory: entry.is_directory(),
        }))
    }
}
