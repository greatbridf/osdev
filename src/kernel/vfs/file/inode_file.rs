use alloc::sync::Arc;

use eonix_sync::Mutex;
use posix_types::getdent::{UserDirent, UserDirent64};
use posix_types::open::OpenFlags;
use posix_types::stat::StatX;

use super::{File, FileType, SeekOption};
use crate::io::{Buffer, BufferFill, Stream};
use crate::kernel::constants::{EBADF, EFAULT, ENOTDIR, EOVERFLOW, ESPIPE};
use crate::kernel::vfs::dentry::Dentry;
use crate::kernel::vfs::inode::{InodeUse, WriteOffset};
use crate::kernel::vfs::types::Format;
use crate::prelude::KResult;

pub struct InodeFile {
    pub r: bool,
    pub w: bool,
    pub a: bool,
    /// Only a few modes those won't possibly change are cached here to speed up file operations.
    /// Specifically, `S_IFMT` masked bits.
    pub format: Format,
    cursor: Mutex<usize>,
    dentry: Arc<Dentry>,
}

impl InodeFile {
    pub fn new(dentry: Arc<Dentry>, flags: OpenFlags) -> File {
        // SAFETY: `dentry` used to create `InodeFile` is valid.
        // SAFETY: `mode` should never change with respect to the `S_IFMT` fields.
        let format = dentry.inode().expect("dentry should be invalid").format;

        let (r, w, a) = flags.as_rwa();

        File::new(
            flags,
            FileType::Inode(InodeFile {
                dentry,
                r,
                w,
                a,
                format,
                cursor: Mutex::new(0),
            }),
        )
    }

    pub fn sendfile_check(&self) -> KResult<()> {
        match self.format {
            Format::REG | Format::BLK => Ok(()),
            _ => Err(EBADF),
        }
    }

    pub async fn write(&self, stream: &mut dyn Stream, offset: Option<usize>) -> KResult<usize> {
        if !self.w {
            return Err(EBADF);
        }

        let mut cursor = self.cursor.lock().await;

        let (offset, update_offset) = match (self.a, offset) {
            (true, _) => (WriteOffset::End(&mut cursor), None),
            (false, Some(offset)) => (WriteOffset::Position(offset), None),
            (false, None) => (WriteOffset::Position(*cursor), Some(&mut *cursor)),
        };

        let nr_write = self.dentry.write(stream, offset).await?;

        if let Some(update_offset) = update_offset {
            *update_offset += nr_write;
        }

        Ok(nr_write)
    }

    pub async fn read(&self, buffer: &mut dyn Buffer, offset: Option<usize>) -> KResult<usize> {
        if !self.r {
            return Err(EBADF);
        }

        if let Some(offset) = offset {
            return Ok(self.dentry.read(buffer, offset).await?);
        }

        let mut cursor = self.cursor.lock().await;
        let nread = self.dentry.read(buffer, *cursor).await?;

        *cursor += nread;
        Ok(nread)
    }
}

impl File {
    pub fn get_inode(&self) -> KResult<Option<InodeUse>> {
        if let FileType::Inode(inode_file) = &**self {
            Ok(Some(inode_file.dentry.get_inode()?))
        } else {
            Ok(None)
        }
    }

    pub async fn getdents(&self, buffer: &mut dyn Buffer) -> KResult<()> {
        let FileType::Inode(inode_file) = &**self else {
            return Err(ENOTDIR);
        };

        let mut cursor = inode_file.cursor.lock().await;

        let nread = inode_file
            .dentry
            .readdir(*cursor, |filename, ino| {
                // + 1 for filename length padding '\0', + 1 for d_type.
                let real_record_len = core::mem::size_of::<UserDirent>() + filename.len() + 2;

                if buffer.available() < real_record_len {
                    return Ok(false);
                }

                let record = UserDirent {
                    d_ino: ino.as_raw() as u32,
                    d_off: 0,
                    d_reclen: real_record_len as u16,
                    d_name: [0; 0],
                };

                buffer.copy(&record)?.ok_or(EFAULT)?;
                buffer.fill(filename)?.ok_or(EFAULT)?;
                buffer.fill(&[0, 0])?.ok_or(EFAULT)?;

                Ok(true)
            })
            .await??;

        *cursor += nread;
        Ok(())
    }

    pub async fn getdents64(&self, buffer: &mut dyn Buffer) -> KResult<()> {
        let FileType::Inode(inode_file) = &**self else {
            return Err(ENOTDIR);
        };

        let mut cursor = inode_file.cursor.lock().await;

        let nread = inode_file
            .dentry
            .readdir(*cursor, |filename, ino| {
                // Filename length + 1 for padding '\0'
                let real_record_len = core::mem::size_of::<UserDirent64>() + filename.len() + 1;

                if buffer.available() < real_record_len {
                    return Ok(false);
                }

                let record = UserDirent64 {
                    d_ino: ino.as_raw(),
                    d_off: 0,
                    d_reclen: real_record_len as u16,
                    d_type: 0,
                    d_name: [0; 0],
                };

                buffer.copy(&record)?.ok_or(EFAULT)?;
                buffer.fill(filename)?.ok_or(EFAULT)?;
                buffer.fill(&[0])?.ok_or(EFAULT)?;

                Ok(true)
            })
            .await??;

        *cursor += nread;
        Ok(())
    }

    pub async fn seek(&self, option: SeekOption) -> KResult<usize> {
        let FileType::Inode(inode_file) = &**self else {
            return Err(ESPIPE);
        };

        let mut cursor = inode_file.cursor.lock().await;

        let new_cursor = match option {
            SeekOption::Current(off) => cursor.checked_add_signed(off).ok_or(EOVERFLOW)?,
            SeekOption::Set(n) => n,
            SeekOption::End(off) => {
                let inode = inode_file.dentry.get_inode()?;
                let size = inode.info.lock().size as usize;
                size.checked_add_signed(off).ok_or(EOVERFLOW)?
            }
        };

        *cursor = new_cursor;
        Ok(new_cursor)
    }

    pub fn statx(&self, buffer: &mut StatX, mask: u32) -> KResult<()> {
        if let FileType::Inode(inode) = &**self {
            inode.dentry.statx(buffer, mask)
        } else {
            Err(EBADF)
        }
    }

    pub fn as_path(&self) -> Option<&Arc<Dentry>> {
        if let FileType::Inode(inode_file) = &**self {
            Some(&inode_file.dentry)
        } else {
            None
        }
    }
}
