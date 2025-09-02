use core::pin::Pin;

use alloc::{boxed::Box, string::String};
use futures::{Stream, StreamExt};
use posix_types::result::PosixError;

use crate::kernel::constants::EINVAL;
use crate::prelude::*;

use super::{Cluster, RawCluster};

#[repr(C, packed)]
pub struct RawDirEntry {
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

pub struct FatDirectoryEntry {
    pub filename: Box<[u8]>,
    pub cluster: Cluster,
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

    fn as_raw_long_filename(&self) -> Option<[u16; 13]> {
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

pub fn as_raw_dirents(data: &[u8]) -> KResult<&[RawDirEntry]> {
    let len = data.len();
    if len % size_of::<RawDirEntry>() != 0 {
        return Err(EINVAL);
    }

    unsafe {
        Ok(core::slice::from_raw_parts(
            data.as_ptr() as *const RawDirEntry,
            len / size_of::<RawDirEntry>(),
        ))
    }
}

pub trait ParseDirent {
    async fn next_dirent(&mut self) -> Option<KResult<FatDirectoryEntry>>;
}

impl<'a, T> ParseDirent for T
where
    T: Stream<Item = KResult<&'a RawDirEntry>>,
{
    async fn next_dirent(&mut self) -> Option<KResult<FatDirectoryEntry>> {
        let mut me = unsafe { Pin::new_unchecked(self) };

        // The long filename entries are stored in reverse order.
        // So we reverse all filename segments and then reverse the whole string at the end.
        let mut filename_rev = String::new();

        let mut is_lfn = false;
        let mut nr_entry_scanned = 0;
        let mut cur_entry;

        loop {
            match me.as_mut().next().await {
                Some(Err(err)) => return Some(Err(err)),
                Some(Ok(ent)) => {
                    cur_entry = ent;
                    nr_entry_scanned += 1;
                }
                None => {
                    if is_lfn {
                        // Unterminated long filename entries are invalid.
                        return Some(Err(PosixError::EINVAL.into()));
                    } else {
                        return None;
                    }
                }
            };

            if !cur_entry.is_invalid() {
                break;
            }

            let Some(raw_long_filename) = cur_entry.as_raw_long_filename() else {
                continue;
            };

            // We are processing a long filename entry.
            is_lfn = true;

            let real_len = raw_long_filename
                .iter()
                .position(|&ch| ch == 0)
                .unwrap_or(raw_long_filename.len());

            let name_codes_rev = raw_long_filename.into_iter().take(real_len).rev();
            let name_chars_rev = char::decode_utf16(name_codes_rev).map(|r| r.unwrap_or('?'));

            filename_rev.extend(name_chars_rev);
        }

        // From now on, `entry` represents a valid directory entry.

        let raw_cluster =
            RawCluster(cur_entry.cluster_low as u32 | ((cur_entry.cluster_high as u32) << 16));

        let Some(cluster) = raw_cluster.parse() else {
            return Some(Err(PosixError::EINVAL.into()));
        };

        let filename;

        if filename_rev.is_empty() {
            let mut name = cur_entry.filename().to_vec();
            let extension = cur_entry.extension();
            if !extension.is_empty() {
                name.push(b'.');
                name.extend_from_slice(extension);
            }

            if cur_entry.is_filename_lowercase() {
                name.make_ascii_lowercase();
            }

            filename = name.into_boxed_slice();
        } else {
            let mut name = filename_rev.into_bytes();
            name.reverse();
            filename = name.into_boxed_slice();
        }

        Some(Ok(FatDirectoryEntry {
            size: cur_entry.size,
            entry_offset: nr_entry_scanned * size_of::<RawDirEntry>() as u32,
            filename,
            cluster,
            is_directory: cur_entry.is_directory(),
        }))
    }
}
