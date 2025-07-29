use super::{
    dentry::Dentry,
    inode::{Mode, WriteOffset},
    s_isblk, s_isreg,
};
use crate::{
    io::{Buffer, BufferFill, ByteBuffer, Chunks, IntoStream},
    kernel::{
        constants::{TCGETS, TCSETS, TIOCGPGRP, TIOCGWINSZ, TIOCSPGRP},
        mem::{paging::Page, AsMemoryBlock as _},
        task::Thread,
        terminal::{Terminal, TerminalIORequest},
        user::{UserPointer, UserPointerMut},
        vfs::inode::Inode,
        CharDevice,
    },
    prelude::*,
    sync::CondVar,
};
use crate::{
    io::{Stream, StreamRead},
    kernel::constants::{
        EBADF, EFAULT, EINTR, EINVAL, ENOTDIR, ENOTTY, EOVERFLOW, EPIPE, ESPIPE, S_IFMT,
    },
};
use alloc::{collections::vec_deque::VecDeque, sync::Arc};
use bitflags::bitflags;
use core::{
    ops::{ControlFlow, Deref},
    sync::atomic::{AtomicU32, Ordering},
};
use eonix_runtime::task::Task;
use eonix_sync::Mutex;
use posix_types::{open::OpenFlags, signal::Signal, stat::StatX};

pub struct InodeFile {
    read: bool,
    write: bool,
    append: bool,
    /// Only a few modes those won't possibly change are cached here to speed up file operations.
    /// Specifically, `S_IFMT` masked bits.
    mode: Mode,
    cursor: Mutex<usize>,
    dentry: Arc<Dentry>,
}

pub struct PipeInner {
    buffer: VecDeque<u8>,
    read_closed: bool,
    write_closed: bool,
}

pub struct Pipe {
    inner: Mutex<PipeInner>,
    cv_read: CondVar,
    cv_write: CondVar,
}

pub struct PipeReadEnd {
    pipe: Arc<Pipe>,
}

pub struct PipeWriteEnd {
    pipe: Arc<Pipe>,
}

pub struct TerminalFile {
    terminal: Arc<Terminal>,
}

// TODO: We should use `File` as the base type, instead of `Arc<File>`
//       If we need shared states, like for `InodeFile`, the files themselves should
//       have their own shared semantics. All `File` variants will just keep the
//       `Clone` semantics.
//
//       e.g. The `CharDevice` itself is stateless.
pub enum FileType {
    Inode(InodeFile),
    PipeRead(PipeReadEnd),
    PipeWrite(PipeWriteEnd),
    TTY(TerminalFile),
    CharDev(Arc<CharDevice>),
}

pub struct File {
    flags: AtomicU32,
    file_type: FileType,
}

impl File {
    pub fn get_inode(&self) -> KResult<Option<Arc<dyn Inode>>> {
        match &self.file_type {
            FileType::Inode(inode_file) => Ok(Some(inode_file.dentry.get_inode()?)),
            _ => Ok(None),
        }
    }
}

pub enum SeekOption {
    Set(usize),
    Current(isize),
    End(isize),
}

bitflags! {
    pub struct PollEvent: u16 {
        const Readable = 0x0001;
        const Writable = 0x0002;
    }
}

impl Drop for PipeReadEnd {
    fn drop(&mut self) {
        self.pipe.close_read();
    }
}

impl Drop for PipeWriteEnd {
    fn drop(&mut self) {
        self.pipe.close_write();
    }
}

fn send_sigpipe_to_current() {
    let current = Thread::current();
    current.raise(Signal::SIGPIPE);
}

impl Pipe {
    const PIPE_SIZE: usize = 4096;

    /// # Return
    /// `(read_end, write_end)`
    pub fn new(flags: OpenFlags) -> (Arc<File>, Arc<File>) {
        let pipe = Arc::new(Self {
            inner: Mutex::new(PipeInner {
                buffer: VecDeque::with_capacity(Self::PIPE_SIZE),
                read_closed: false,
                write_closed: false,
            }),
            cv_read: CondVar::new(),
            cv_write: CondVar::new(),
        });

        let read_flags = flags.difference(OpenFlags::O_WRONLY | OpenFlags::O_RDWR);
        let mut write_flags = read_flags;
        write_flags.insert(OpenFlags::O_WRONLY);

        (
            Arc::new(File {
                flags: AtomicU32::new(read_flags.bits()),
                file_type: FileType::PipeRead(PipeReadEnd { pipe: pipe.clone() }),
            }),
            Arc::new(File {
                flags: AtomicU32::new(write_flags.bits()),
                file_type: FileType::PipeWrite(PipeWriteEnd { pipe }),
            }),
        )
    }

    fn close_read(&self) {
        let mut inner = Task::block_on(self.inner.lock());
        if inner.read_closed {
            return;
        }

        inner.read_closed = true;
        self.cv_write.notify_all();
    }

    fn close_write(&self) {
        let mut inner = Task::block_on(self.inner.lock());
        if inner.write_closed {
            return;
        }

        inner.write_closed = true;
        self.cv_read.notify_all();
    }

    async fn poll(&self, event: PollEvent) -> KResult<PollEvent> {
        if !event.contains(PollEvent::Readable) {
            unimplemented!("Poll event not supported.");
        }

        let mut inner = self.inner.lock().await;
        while inner.buffer.is_empty() && !inner.write_closed {
            inner = self.cv_read.wait(inner).await;
        }

        if Thread::current().signal_list.has_pending_signal() {
            return Err(EINTR);
        }

        let mut retval = PollEvent::empty();
        if inner.write_closed {
            retval |= PollEvent::Writable;
        }

        if !inner.buffer.is_empty() {
            retval |= PollEvent::Readable;
        }

        Ok(retval)
    }

    async fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        let mut inner = self.inner.lock().await;

        while !inner.write_closed && inner.buffer.is_empty() {
            inner = self.cv_read.wait(inner).await;
            if Thread::current().signal_list.has_pending_signal() {
                return Err(EINTR);
            }
        }

        let (data1, data2) = inner.buffer.as_slices();
        let nread = buffer.fill(data1)?.allow_partial() + buffer.fill(data2)?.allow_partial();
        inner.buffer.drain(..nread);

        self.cv_write.notify_all();
        Ok(nread)
    }

    async fn write_atomic(&self, data: &[u8]) -> KResult<usize> {
        let mut inner = self.inner.lock().await;

        if inner.read_closed {
            send_sigpipe_to_current();
            return Err(EPIPE);
        }

        while inner.buffer.len() + data.len() > Self::PIPE_SIZE {
            inner = self.cv_write.wait(inner).await;
            if Thread::current().signal_list.has_pending_signal() {
                return Err(EINTR);
            }

            if inner.read_closed {
                send_sigpipe_to_current();
                return Err(EPIPE);
            }
        }

        inner.buffer.extend(data);

        self.cv_read.notify_all();
        return Ok(data.len());
    }

    async fn write(&self, stream: &mut dyn Stream) -> KResult<usize> {
        let mut buffer = [0; Self::PIPE_SIZE];
        let mut total = 0;
        while let Some(data) = stream.poll_data(&mut buffer)? {
            let nwrote = self.write_atomic(data).await?;
            total += nwrote;
            if nwrote != data.len() {
                break;
            }
        }
        Ok(total)
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
struct UserDirent64 {
    /// Inode number
    d_ino: u64,
    /// Implementation defined. We ignore it
    d_off: u64,
    /// Length of this record
    d_reclen: u16,
    /// File type. Set to 0
    d_type: u8,
    /// Filename with a padding '\0'
    d_name: [u8; 0],
}

/// File type is at offset `d_reclen - 1`. Set it to 0
#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
struct UserDirent {
    /// Inode number
    d_ino: u32,
    /// Implementation defined. We ignore it
    d_off: u32,
    /// Length of this record
    d_reclen: u16,
    /// Filename with a padding '\0'
    d_name: [u8; 0],
}

impl InodeFile {
    pub fn new(dentry: Arc<Dentry>, flags: OpenFlags) -> Arc<File> {
        // SAFETY: `dentry` used to create `InodeFile` is valid.
        // SAFETY: `mode` should never change with respect to the `S_IFMT` fields.
        let cached_mode = dentry
            .get_inode()
            .expect("`dentry` is invalid")
            .mode
            .load(Ordering::Relaxed)
            & S_IFMT;

        let (read, write, append) = flags.as_rwa();

        Arc::new(File {
            flags: AtomicU32::new(flags.bits()),
            file_type: FileType::Inode(InodeFile {
                dentry,
                read,
                write,
                append,
                mode: cached_mode,
                cursor: Mutex::new(0),
            }),
        })
    }

    fn seek(&self, option: SeekOption) -> KResult<usize> {
        let mut cursor = Task::block_on(self.cursor.lock());

        let new_cursor = match option {
            SeekOption::Current(off) => cursor.checked_add_signed(off).ok_or(EOVERFLOW)?,
            SeekOption::Set(n) => n,
            SeekOption::End(off) => {
                let inode = self.dentry.get_inode()?;
                let size = inode.size.load(Ordering::Relaxed) as usize;
                size.checked_add_signed(off).ok_or(EOVERFLOW)?
            }
        };

        *cursor = new_cursor;
        Ok(new_cursor)
    }

    fn write(&self, stream: &mut dyn Stream, offset: Option<usize>) -> KResult<usize> {
        if !self.write {
            return Err(EBADF);
        }

        let mut cursor = Task::block_on(self.cursor.lock());

        if self.append {
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

    fn read(&self, buffer: &mut dyn Buffer, offset: Option<usize>) -> KResult<usize> {
        if !self.read {
            return Err(EBADF);
        }

        let nread = if let Some(offset) = offset {
            let nread = self.dentry.read(buffer, offset)?;
            nread
        } else {
            let mut cursor = Task::block_on(self.cursor.lock());

            let nread = self.dentry.read(buffer, *cursor)?;

            *cursor += nread;
            nread
        };

        Ok(nread)
    }

    fn getdents64(&self, buffer: &mut dyn Buffer) -> KResult<()> {
        let mut cursor = Task::block_on(self.cursor.lock());

        let nread = self.dentry.readdir(*cursor, |filename, ino| {
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

    fn getdents(&self, buffer: &mut dyn Buffer) -> KResult<()> {
        let mut cursor = Task::block_on(self.cursor.lock());

        let nread = self.dentry.readdir(*cursor, |filename, ino| {
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
}

impl TerminalFile {
    pub fn new(tty: Arc<Terminal>, flags: OpenFlags) -> Arc<File> {
        Arc::new(File {
            flags: AtomicU32::new(flags.bits()),
            file_type: FileType::TTY(TerminalFile { terminal: tty }),
        })
    }

    async fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        self.terminal.read(buffer).await
    }

    fn write(&self, stream: &mut dyn Stream) -> KResult<usize> {
        stream.read_till_end(&mut [0; 128], |data| {
            self.terminal.write(data);
            Ok(())
        })
    }

    async fn poll(&self, event: PollEvent) -> KResult<PollEvent> {
        if !event.contains(PollEvent::Readable) {
            unimplemented!("Poll event not supported.")
        }

        self.terminal.poll_in().await.map(|_| PollEvent::Readable)
    }

    fn ioctl(&self, request: usize, arg3: usize) -> KResult<()> {
        Task::block_on(self.terminal.ioctl(match request as u32 {
            TCGETS => TerminalIORequest::GetTermios(UserPointerMut::new_vaddr(arg3)?),
            TCSETS => TerminalIORequest::SetTermios(UserPointer::new_vaddr(arg3)?),
            TIOCGPGRP => TerminalIORequest::GetProcessGroup(UserPointerMut::new_vaddr(arg3)?),
            TIOCSPGRP => TerminalIORequest::SetProcessGroup(UserPointer::new_vaddr(arg3)?),
            TIOCGWINSZ => TerminalIORequest::GetWindowSize(UserPointerMut::new_vaddr(arg3)?),
            _ => return Err(EINVAL),
        }))
    }
}

impl FileType {
    pub async fn read(&self, buffer: &mut dyn Buffer, offset: Option<usize>) -> KResult<usize> {
        match self {
            FileType::Inode(inode) => inode.read(buffer, offset),
            FileType::PipeRead(pipe) => pipe.pipe.read(buffer).await,
            FileType::TTY(tty) => tty.read(buffer).await,
            FileType::CharDev(device) => device.read(buffer),
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
            FileType::Inode(inode) => inode.write(stream, offset),
            FileType::PipeWrite(pipe) => pipe.pipe.write(stream).await,
            FileType::TTY(tty) => tty.write(stream),
            FileType::CharDev(device) => device.write(stream),
            _ => Err(EBADF),
        }
    }

    pub fn seek(&self, option: SeekOption) -> KResult<usize> {
        match self {
            FileType::Inode(inode) => inode.seek(option),
            _ => Err(ESPIPE),
        }
    }

    pub fn getdents(&self, buffer: &mut dyn Buffer) -> KResult<()> {
        match self {
            FileType::Inode(inode) => inode.getdents(buffer),
            _ => Err(ENOTDIR),
        }
    }

    pub fn getdents64(&self, buffer: &mut dyn Buffer) -> KResult<()> {
        match self {
            FileType::Inode(inode) => inode.getdents64(buffer),
            _ => Err(ENOTDIR),
        }
    }

    pub async fn sendfile(&self, dest_file: &Self, count: usize) -> KResult<usize> {
        let buffer_page = Page::alloc();
        // SAFETY: We are the only owner of the page.
        let buffer = unsafe { buffer_page.as_memblk().as_bytes_mut() };

        match self {
            FileType::Inode(file) if s_isblk(file.mode) || s_isreg(file.mode) => (),
            _ => return Err(EINVAL),
        }

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

    pub fn ioctl(&self, request: usize, arg3: usize) -> KResult<usize> {
        match self {
            FileType::TTY(tty) => tty.ioctl(request, arg3).map(|_| 0),
            _ => Err(ENOTTY),
        }
    }

    pub async fn poll(&self, event: PollEvent) -> KResult<PollEvent> {
        match self {
            FileType::Inode(_) => Ok(event),
            FileType::TTY(tty) => tty.poll(event).await,
            FileType::PipeRead(PipeReadEnd { pipe })
            | FileType::PipeWrite(PipeWriteEnd { pipe }) => pipe.poll(event).await,
            _ => unimplemented!("Poll event not supported."),
        }
    }

    pub fn statx(&self, buffer: &mut StatX, mask: u32) -> KResult<()> {
        match self {
            FileType::Inode(inode) => inode.dentry.statx(buffer, mask),
            _ => Err(EBADF),
        }
    }

    pub fn as_path(&self) -> Option<&Arc<Dentry>> {
        match self {
            FileType::Inode(inode_file) => Some(&inode_file.dentry),
            _ => None,
        }
    }
}

impl File {
    pub fn new(flags: OpenFlags, file_type: FileType) -> Arc<Self> {
        Arc::new(Self {
            flags: AtomicU32::new(flags.bits()),
            file_type,
        })
    }

    pub fn get_flags(&self) -> OpenFlags {
        OpenFlags::from_bits_retain(self.flags.load(Ordering::Relaxed))
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

        self.flags.store(flags.bits(), Ordering::Relaxed);
    }
}

impl Deref for File {
    type Target = FileType;

    fn deref(&self) -> &Self::Target {
        &self.file_type
    }
}
