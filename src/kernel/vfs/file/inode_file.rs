use super::{File, FileType, SeekOption};
use crate::{
    io::{Buffer, BufferFill, Stream},
    kernel::{
        constants::{EBADF, EFAULT, ENOTDIR, EOVERFLOW, ESPIPE},
        vfs::{
            dentry::Dentry,
            inode::{Inode, Mode, WriteOffset},
        },
    },
    prelude::KResult,
};
use alloc::sync::Arc;
use core::{ops::ControlFlow, sync::atomic::Ordering};
use eonix_sync::Mutex;
use posix_types::{
    getdent::{UserDirent, UserDirent64},
    open::OpenFlags,
    stat::StatX,
};

pub struct InodeFile {
    pub r: bool,
    pub w: bool,
    pub a: bool,
    /// Only a few modes those won't possibly change are cached here to speed up file operations.
    /// Specifically, `S_IFMT` masked bits.
    pub mode: Mode,
    cursor: Mutex<usize>,
    dentry: Arc<Dentry>,
}

impl InodeFile {
    pub fn new(dentry: Arc<Dentry>, flags: OpenFlags) -> File {
        // SAFETY: `dentry` used to create `InodeFile` is valid.
        // SAFETY: `mode` should never change with respect to the `S_IFMT` fields.
        let cached_mode = dentry
            .get_inode()
            .expect("`dentry` is invalid")
            .mode
            .load()
            .format();

        let (r, w, a) = flags.as_rwa();

        File::new(
            flags,
            FileType::Inode(InodeFile {
                dentry,
                r,
                w,
                a,
                mode: cached_mode,
                cursor: Mutex::new(0),
            }),
        )
    }

    pub fn sendfile_check(&self) -> KResult<()> {
        match self.mode {
            Mode::REG | Mode::BLK => Ok(()),
            _ => Err(EBADF),
        }
    }

    pub async fn write(&self, stream: &mut dyn Stream, offset: Option<usize>) -> KResult<usize> {
        if !self.w {
            return Err(EBADF);
        }

        let mut cursor = self.cursor.lock().await;

        if self.a {
            let nwrote = self.dentry.write(stream, WriteOffset::End(&mut cursor))?;

            Ok(nwrote)
        } else {
            let nwrote = if let Some(offset) = offset {
                self.dentry.write(stream, WriteOffset::Position(offset))?
            } else {
                let nwrote = self.dentry.write(stream, WriteOffset::Position(*cursor))?;
                *cursor += nwrote;
                nwrote
            };

            Ok(nwrote)
        }
    }

    pub async fn read(&self, buffer: &mut dyn Buffer, offset: Option<usize>) -> KResult<usize> {
        if !self.r {
            return Err(EBADF);
        }

        let nread = if let Some(offset) = offset {
            let nread = self.dentry.read(buffer, offset)?;
            nread
        } else {
            let mut cursor = self.cursor.lock().await;

            let nread = self.dentry.read(buffer, *cursor)?;

            *cursor += nread;
            nread
        };

        Ok(nread)
    }

    pub fn size(&self) -> usize {
        self.dentry.size()
    }

    pub fn truncate(&self, new_size: usize) -> KResult<()> {
        self.dentry.truncate(new_size)
    }
}

impl File {
    pub fn get_inode(&self) -> KResult<Option<Arc<dyn Inode>>> {
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

        let nread = inode_file.dentry.readdir(*cursor, |filename, ino| {
            // + 1 for filename length padding '\0', + 1 for d_type.
            let real_record_len = core::mem::size_of::<UserDirent>() + filename.len() + 2;

            if buffer.available() < real_record_len {
                return Ok(ControlFlow::Break(()));
            }

            let record = UserDirent {
                d_ino: ino as u32,
                d_off: 0,
                d_reclen: real_record_len as u16,
                d_name: [0; 0],
            };

            buffer.copy(&record)?.ok_or(EFAULT)?;
            buffer.fill(filename)?.ok_or(EFAULT)?;
            buffer.fill(&[0, 0])?.ok_or(EFAULT)?;

            Ok(ControlFlow::Continue(()))
        })?;

        *cursor += nread;
        Ok(())
    }

    pub async fn getdents64(&self, buffer: &mut dyn Buffer) -> KResult<()> {
        let FileType::Inode(inode_file) = &**self else {
            return Err(ENOTDIR);
        };

        let mut cursor = inode_file.cursor.lock().await;

        let nread = inode_file.dentry.readdir(*cursor, |filename, ino| {
            // Filename length + 1 for padding '\0'
            let real_record_len = core::mem::size_of::<UserDirent64>() + filename.len() + 1;

            if buffer.available() < real_record_len {
                return Ok(ControlFlow::Break(()));
            }

            let record = UserDirent64 {
                d_ino: ino,
                d_off: 0,
                d_reclen: real_record_len as u16,
                d_type: 0,
                d_name: [0; 0],
            };

            buffer.copy(&record)?.ok_or(EFAULT)?;
            buffer.fill(filename)?.ok_or(EFAULT)?;
            buffer.fill(&[0])?.ok_or(EFAULT)?;

            Ok(ControlFlow::Continue(()))
        })?;

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
                let size = inode.size.load(Ordering::Relaxed) as usize;
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
