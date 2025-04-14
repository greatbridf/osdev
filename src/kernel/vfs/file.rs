use super::{
    dentry::Dentry,
    inode::{Mode, WriteOffset},
    s_isblk, s_isdir, s_isreg,
};
use crate::{
    io::{Buffer, BufferFill, ByteBuffer},
    kernel::{
        constants::{TCGETS, TCSETS, TIOCGPGRP, TIOCGWINSZ, TIOCSPGRP},
        mem::paging::Page,
        task::{Signal, Thread},
        terminal::{Terminal, TerminalIORequest},
        user::{UserPointer, UserPointerMut},
        CharDevice,
    },
    prelude::*,
    sync::CondVar,
};
use alloc::{collections::vec_deque::VecDeque, sync::Arc};
use bindings::{
    statx, EBADF, EFAULT, EINTR, EINVAL, ENOTDIR, ENOTTY, EOVERFLOW, EPIPE, ESPIPE, S_IFMT,
};
use bitflags::bitflags;
use core::{ops::ControlFlow, sync::atomic::Ordering};
use eonix_runtime::task::Task;
use eonix_sync::Mutex;

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
pub enum File {
    Inode(InodeFile),
    PipeRead(PipeReadEnd),
    PipeWrite(PipeWriteEnd),
    TTY(TerminalFile),
    CharDev(Arc<CharDevice>),
}

pub enum SeekOption {
    Set(usize),
    Current(isize),
    End(isize),
}

bitflags! {
    pub struct PollEvent: u16 {
        const Readable = 0x0001;
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
    // SAFETY: current_thread is always valid.
    let current = Thread::current();
    current.raise(Signal::SIGPIPE);
}

impl Pipe {
    const PIPE_SIZE: usize = 4096;

    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(PipeInner {
                buffer: VecDeque::with_capacity(Self::PIPE_SIZE),
                read_closed: false,
                write_closed: false,
            }),
            cv_read: CondVar::new(),
            cv_write: CondVar::new(),
        })
    }

    /// # Return
    /// `(read_end, write_end)`
    pub fn split(self: &Arc<Self>) -> (Arc<File>, Arc<File>) {
        (
            Arc::new(File::PipeRead(PipeReadEnd { pipe: self.clone() })),
            Arc::new(File::PipeWrite(PipeWriteEnd { pipe: self.clone() })),
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

    async fn write_non_atomic(&self, data: &[u8]) -> KResult<usize> {
        let mut inner = self.inner.lock().await;

        if inner.read_closed {
            send_sigpipe_to_current();
            return Err(EPIPE);
        }

        let mut remaining = data;
        while !remaining.is_empty() {
            let space = inner.buffer.capacity() - inner.buffer.len();

            if space != 0 {
                let to_write = remaining.len().min(space);
                inner.buffer.extend(&remaining[..to_write]);
                remaining = &remaining[to_write..];

                self.cv_read.notify_all();
            }

            if remaining.is_empty() {
                break;
            }

            inner = self.cv_write.wait(inner).await;
            if Thread::current().signal_list.has_pending_signal() {
                if data.len() != remaining.len() {
                    break;
                }
                return Err(EINTR);
            }

            if inner.read_closed {
                send_sigpipe_to_current();
                return Err(EPIPE);
            }
        }

        Ok(data.len() - remaining.len())
    }

    async fn write(&self, data: &[u8]) -> KResult<usize> {
        // Writes those are smaller than the pipe size are atomic.
        if data.len() <= Self::PIPE_SIZE {
            self.write_atomic(data).await
        } else {
            self.write_non_atomic(data).await
        }
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
    pub fn new(dentry: Arc<Dentry>, rwa: (bool, bool, bool)) -> Arc<File> {
        // SAFETY: `dentry` used to create `InodeFile` is valid.
        // SAFETY: `mode` should never change with respect to the `S_IFMT` fields.
        let cached_mode = dentry
            .get_inode()
            .expect("`dentry` is invalid")
            .mode
            .load(Ordering::Relaxed)
            & S_IFMT;

        Arc::new(File::Inode(InodeFile {
            dentry,
            read: rwa.0,
            write: rwa.1,
            append: rwa.2,
            mode: cached_mode,
            cursor: Mutex::new(0),
        }))
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

    fn write(&self, buffer: &[u8]) -> KResult<usize> {
        if !self.write {
            return Err(EBADF);
        }

        let mut cursor = Task::block_on(self.cursor.lock());

        // TODO!!!: use `UserBuffer`
        if self.append {
            let nwrote = self.dentry.write(buffer, WriteOffset::End(&mut cursor))?;

            Ok(nwrote)
        } else {
            let nwrote = self.dentry.write(buffer, WriteOffset::Position(*cursor))?;

            *cursor += nwrote;
            Ok(nwrote)
        }
    }

    fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        if !self.read {
            return Err(EBADF);
        }

        let mut cursor = Task::block_on(self.cursor.lock());

        let nread = self.dentry.read(buffer, *cursor)?;

        *cursor += nread;
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
    pub fn new(tty: Arc<Terminal>) -> Arc<File> {
        Arc::new(File::TTY(TerminalFile { terminal: tty }))
    }

    async fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        self.terminal.read(buffer).await
    }

    fn write(&self, buffer: &[u8]) -> KResult<usize> {
        for &ch in buffer.iter() {
            self.terminal.show_char(ch);
        }

        Ok(buffer.len())
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

impl File {
    pub async fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        match self {
            File::Inode(inode) => inode.read(buffer),
            File::PipeRead(pipe) => pipe.pipe.read(buffer).await,
            File::TTY(tty) => tty.read(buffer).await,
            File::CharDev(device) => device.read(buffer),
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

    pub async fn write(&self, buffer: &[u8]) -> KResult<usize> {
        match self {
            File::Inode(inode) => inode.write(buffer),
            File::PipeWrite(pipe) => pipe.pipe.write(buffer).await,
            File::TTY(tty) => tty.write(buffer),
            File::CharDev(device) => device.write(buffer),
            _ => Err(EBADF),
        }
    }

    pub fn seek(&self, option: SeekOption) -> KResult<usize> {
        match self {
            File::Inode(inode) => inode.seek(option),
            _ => Err(ESPIPE),
        }
    }

    pub fn getdents(&self, buffer: &mut dyn Buffer) -> KResult<()> {
        match self {
            File::Inode(inode) => inode.getdents(buffer),
            _ => Err(ENOTDIR),
        }
    }

    pub fn getdents64(&self, buffer: &mut dyn Buffer) -> KResult<()> {
        match self {
            File::Inode(inode) => inode.getdents64(buffer),
            _ => Err(ENOTDIR),
        }
    }

    pub async fn sendfile(&self, dest_file: &Self, count: usize) -> KResult<usize> {
        let buffer_page = Page::alloc_one();

        match self {
            File::Inode(file) if s_isblk(file.mode) || s_isreg(file.mode) => (),
            _ => return Err(EINVAL),
        }

        // TODO!!!: zero copy implementation with mmap
        let mut tot = 0usize;
        while tot < count {
            if Thread::current().signal_list.has_pending_signal() {
                if tot == 0 {
                    return Err(EINTR);
                } else {
                    return Ok(tot);
                }
            }

            let batch_size = usize::min(count - tot, buffer_page.len());
            let slice = &mut buffer_page.as_mut_slice()[..batch_size];
            let mut buffer = ByteBuffer::new(slice);

            let nwrote = self.read(&mut buffer).await?;

            if nwrote == 0 {
                break;
            }

            tot += dest_file.write(&slice[..nwrote]).await?;
        }

        Ok(tot)
    }

    pub fn ioctl(&self, request: usize, arg3: usize) -> KResult<usize> {
        match self {
            File::TTY(tty) => tty.ioctl(request, arg3).map(|_| 0),
            _ => Err(ENOTTY),
        }
    }

    pub async fn poll(&self, event: PollEvent) -> KResult<PollEvent> {
        match self {
            File::Inode(_) => Ok(event),
            File::TTY(tty) => tty.poll(event).await,
            _ => unimplemented!("Poll event not supported."),
        }
    }

    pub fn statx(&self, buffer: &mut statx, mask: u32) -> KResult<()> {
        match self {
            File::Inode(inode) => inode.dentry.statx(buffer, mask),
            _ => Err(EBADF),
        }
    }

    pub fn as_path(&self) -> Option<&Arc<Dentry>> {
        match self {
            File::Inode(inode_file) if s_isdir(inode_file.mode) => Some(&inode_file.dentry),
            _ => None,
        }
    }
}
