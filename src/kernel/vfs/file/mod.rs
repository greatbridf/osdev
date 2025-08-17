mod inode_file;
mod pipe;
mod terminal_file;

use crate::{
    io::{Buffer, ByteBuffer, Chunks, IntoStream, Stream},
    kernel::{
        constants::{EBADF, EINTR, EINVAL, ENOTTY},
        mem::{AsMemoryBlock, Page},
        task::Thread,
        CharDevice,
    },
    prelude::KResult,
    net::socket::{SendMetadata, Socket},
};
use alloc::sync::Arc;
use bitflags::bitflags;
use core::{
    ops::Deref,
    sync::atomic::{AtomicI32, AtomicU32, Ordering},
};
use pipe::{PipeReadEnd, PipeWriteEnd};
use posix_types::open::OpenFlags;

pub use inode_file::InodeFile;
pub use pipe::Pipe;
pub use terminal_file::TerminalFile;

pub enum FileType {
    Inode(InodeFile),
    PipeRead(PipeReadEnd),
    PipeWrite(PipeWriteEnd),
    Terminal(TerminalFile),
    CharDev(Arc<CharDevice>),
    Socket(Arc<dyn Socket>),
}

struct FileData {
    flags: AtomicU32,
    open_count: AtomicI32,
    file_type: FileType,
}

#[derive(Clone)]
pub struct File(Arc<FileData>);

pub enum SeekOption {
    Set(usize),
    Current(isize),
    End(isize),
}

bitflags! {
    #[derive(Clone, Copy, Debug)]
    pub struct PollEvent: u16 {
        const Readable = 0x0001;
        const Writable = 0x0002;
    }
}

impl FileType {
    pub async fn read(&self, buffer: &mut dyn Buffer, offset: Option<usize>) -> KResult<usize> {
        match self {
            FileType::Inode(inode) => inode.read(buffer, offset).await,
            FileType::PipeRead(pipe) => pipe.read(buffer).await,
            FileType::Terminal(tty) => tty.read(buffer).await,
            FileType::CharDev(device) => device.read(buffer),
            FileType::Socket(socket) => socket.recv(buffer).await.map(|res| res.0),
            _ => Err(EBADF),
        }
    }

    // TODO
    // /// Read from the file into the given buffers.
    // ///
    // /// Reads are atomic, not intermingled with other reads or writes.
    // pub fn readv<'r, 'i, I: Iterator<Item = &'i mut dyn Buffer>>(
    //     &'r self,
    //     buffers: I,
    // ) -> KResult<usize> {
    //     match self {
    //         File::Inode(inode) => inode.readv(buffers),
    //         File::PipeRead(pipe) => pipe.pipe.readv(buffers),
    //         _ => Err(EBADF),
    //     }
    // }

    pub async fn write(&self, stream: &mut dyn Stream, offset: Option<usize>) -> KResult<usize> {
        match self {
            FileType::Inode(inode) => inode.write(stream, offset).await,
            FileType::PipeWrite(pipe) => pipe.write(stream).await,
            FileType::Terminal(tty) => tty.write(stream),
            FileType::CharDev(device) => device.write(stream),
            FileType::Socket(socket) => socket.send(stream, SendMetadata::default()).await,
            _ => Err(EBADF),
        }
    }

    fn sendfile_check(&self) -> KResult<()> {
        match self {
            FileType::Inode(file) => file.sendfile_check(),
            _ => Err(EINVAL),
        }
    }

    pub async fn sendfile(&self, dest_file: &Self, count: usize) -> KResult<usize> {
        let buffer_page = Page::alloc();
        // SAFETY: We are the only owner of the page.
        let buffer = unsafe { buffer_page.as_memblk().as_bytes_mut() };

        self.sendfile_check()?;

        let mut nsent = 0;
        for (cur, len) in Chunks::new(0, count, buffer.len()) {
            if Thread::current().signal_list.has_pending_signal() {
                return if cur == 0 { Err(EINTR) } else { Ok(cur) };
            }
            let nread = self
                .read(&mut ByteBuffer::new(&mut buffer[..len]), None)
                .await?;
            if nread == 0 {
                break;
            }

            let nwrote = dest_file
                .write(&mut buffer[..nread].into_stream(), None)
                .await?;
            nsent += nwrote;

            if nwrote != len {
                break;
            }
        }

        Ok(nsent)
    }

    pub async fn ioctl(&self, request: usize, arg3: usize) -> KResult<usize> {
        match self {
            FileType::Terminal(tty) => tty.ioctl(request, arg3).await.map(|_| 0),
            _ => Err(ENOTTY),
        }
    }

    pub async fn poll(&self, event: PollEvent) -> KResult<PollEvent> {
        match self {
            FileType::Inode(_) => Ok(event),
            FileType::Terminal(tty) => tty.poll(event).await,
            FileType::PipeRead(pipe) => pipe.poll(event).await,
            FileType::PipeWrite(pipe) => pipe.poll(event).await,
            FileType::Socket(socket) => socket.poll(event),
            _ => unimplemented!("Poll event not supported."),
        }
    }

    pub fn size(&self) -> usize {
        match self {
            FileType::Inode(inode_file) => inode_file.size(),
            _ => panic!("Only InodeFile have size."),
        }
    }

    pub fn truncate(&self, new_size: usize) -> KResult<()> {
        match self {
            FileType::Inode(inode_file) => inode_file.truncate(new_size),
            _ => panic!("Only InodeFile can truncate."),
        }
    }
}

impl File {
    pub fn new(flags: OpenFlags, file_type: FileType) -> Self {
        Self(Arc::new(FileData {
            flags: AtomicU32::new(flags.bits()),
            open_count: AtomicI32::new(1),
            file_type,
        }))
    }

    pub fn get_flags(&self) -> OpenFlags {
        OpenFlags::from_bits_retain(self.0.flags.load(Ordering::Relaxed))
    }

    pub fn set_flags(&self, flags: OpenFlags) {
        let flags = flags.difference(
            OpenFlags::O_WRONLY
                | OpenFlags::O_RDWR
                | OpenFlags::O_CREAT
                | OpenFlags::O_TRUNC
                | OpenFlags::O_EXCL,
            // | OpenFlags::O_NOCTTY,
        );

        self.0.flags.store(flags.bits(), Ordering::Relaxed);
    }

    /// Duplicate the file descriptor in order to store it in some [FileArray].
    ///
    /// The [`File`]s stored in [FileArray]s hold an "open count", which is used
    /// to track how many references to the file are currently open.
    ///
    /// # Panics
    /// The [`File`]s stored in [FileArray]s MUST be retrieved by calling this
    /// method. Otherwise, when the last reference to the file is dropped,
    /// something bad will happen. ;)
    ///
    /// [FileArray]: crate::kernel::vfs::filearray::FileArray
    pub fn dup(&self) -> Self {
        self.0.open_count.fetch_add(1, Ordering::Relaxed);
        Self(self.0.clone())
    }

    /// Close the file descriptor, decrementing the open count.
    pub async fn close(self) {
        // Due to rust async drop limits, we have to do this manually...
        //
        // Users of files can clone and drop it freely, but references held by
        // file arrays must be dropped by calling this function (in order to
        // await for the async close operation of the inner FileType).
        match self.0.open_count.fetch_sub(1, Ordering::Relaxed) {
            ..1 => panic!("File open count underflow."),
            1 => {}
            _ => return,
        }

        match &self.0.file_type {
            FileType::PipeRead(pipe) => pipe.close().await,
            FileType::PipeWrite(pipe) => pipe.close().await,
            _ => {}
        }
    }
    
    pub fn get_socket(&self) -> KResult<Option<Arc<dyn Socket>>> {
        match &self.0.file_type {
            FileType::Socket(socket) => Ok(Some(socket.clone())),
            _ => Ok(None),
        }
    }
}

impl Drop for FileData {
    fn drop(&mut self) {
        // If you're "lucky" enough to see this, it means that you've violated
        // the file reference counting rules. Check File::close() for details. ;)
        assert_eq!(
            self.open_count.load(Ordering::Relaxed),
            0,
            "File dropped with open count 0, check the comments for details."
        );
    }
}

impl Deref for File {
    type Target = FileType;

    fn deref(&self) -> &Self::Target {
        &self.0.file_type
    }
}
