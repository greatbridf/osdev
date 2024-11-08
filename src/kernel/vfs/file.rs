use core::{
    ffi::{c_int, c_ulong},
    ops::ControlFlow,
    ptr::NonNull,
    sync::atomic::Ordering,
};

use crate::{
    io::{Buffer, BufferFill, RawBuffer},
    kernel::mem::{paging::Page, phys::PhysPtr},
    prelude::*,
    sync::condvar::CondVar,
};

use alloc::{collections::vec_deque::VecDeque, sync::Arc};
use bindings::{
    current_thread, kernel::tty::tty as TTY, EBADF, EFAULT, EINTR, EINVAL, ENOTDIR, ENOTTY,
    EOVERFLOW, EPIPE, ESPIPE, SIGPIPE, S_IFMT,
};

use super::{
    dentry::Dentry,
    inode::{Mode, WriteOffset},
    s_isblk, s_isreg,
};

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
    inner: Spin<PipeInner>,
    cv_read: CondVar,
    cv_write: CondVar,
}

pub struct PipeReadEnd {
    pipe: Arc<Pipe>,
}

pub struct PipeWriteEnd {
    pipe: Arc<Pipe>,
}

pub struct TTYFile {
    tty: NonNull<TTY>,
}

pub enum File {
    Inode(InodeFile),
    PipeRead(PipeReadEnd),
    PipeWrite(PipeWriteEnd),
    TTY(TTYFile),
}

pub enum SeekOption {
    Set(usize),
    Current(isize),
    End(isize),
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
    // Safety: current_thread is always valid.
    let current = unsafe { current_thread.as_mut().unwrap() };

    // Safety: `signal_list` is `Sync`
    unsafe { current.send_signal(SIGPIPE) };
}

impl Pipe {
    const PIPE_SIZE: usize = 4096;

    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Spin::new(PipeInner {
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
        let mut inner = self.inner.lock();
        if inner.read_closed {
            return;
        }

        inner.read_closed = true;
        self.cv_write.notify_all();
    }

    fn close_write(&self) {
        let mut inner = self.inner.lock();
        if inner.write_closed {
            return;
        }

        inner.write_closed = true;
        self.cv_read.notify_all();
    }

    fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        let mut inner = self.inner.lock();

        while !inner.write_closed && inner.buffer.is_empty() {
            let interrupted = self.cv_read.wait(&mut inner, true);
            if interrupted {
                return Err(EINTR);
            }
        }

        let (data1, data2) = inner.buffer.as_slices();
        let nread = buffer.fill(data1)?.allow_partial() + buffer.fill(data2)?.allow_partial();
        inner.buffer.drain(..nread);

        self.cv_write.notify_all();
        Ok(nread)
    }

    fn write_atomic(&self, data: &[u8]) -> KResult<usize> {
        let mut inner = self.inner.lock();

        if inner.read_closed {
            send_sigpipe_to_current();
            return Err(EPIPE);
        }

        while inner.buffer.len() + data.len() > Self::PIPE_SIZE {
            let interrupted = self.cv_write.wait(&mut inner, true);
            if interrupted {
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

    fn write_non_atomic(&self, data: &[u8]) -> KResult<usize> {
        let mut inner = self.inner.lock();

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

            let interrupted = self.cv_write.wait(&mut inner, true);
            if interrupted {
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

    fn write(&self, data: &[u8]) -> KResult<usize> {
        // Writes those are smaller than the pipe size are atomic.
        if data.len() <= Self::PIPE_SIZE {
            self.write_atomic(data)
        } else {
            self.write_non_atomic(data)
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

fn has_pending_signal() -> bool {
    unsafe { current_thread.as_mut().unwrap().signals.pending_signal() != 0 }
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
        let mut cursor = self.cursor.lock();

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

        let mut cursor = self.cursor.lock();

        // TODO!!!: use `UserBuffer`
        if self.append {
            let nwrote = self
                .dentry
                .write(buffer, WriteOffset::End(cursor.as_mut()))?;

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

        let mut cursor = self.cursor.lock();

        let nread = self.dentry.read(buffer, *cursor)?;

        *cursor += nread;
        Ok(nread)
    }

    fn getdents64(&self, buffer: &mut dyn Buffer) -> KResult<()> {
        let mut cursor = self.cursor.lock();

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
        let mut cursor = self.cursor.lock();

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

impl TTYFile {
    pub fn new(tty: *mut TTY) -> Arc<File> {
        Arc::new(File::TTY(TTYFile {
            tty: NonNull::new(tty).expect("`tty` is null"),
        }))
    }

    fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        // SAFETY: `tty` should always valid.
        let tty = unsafe { self.tty.as_ptr().as_mut().unwrap() };

        let mut c_buffer: Vec<u8> = vec![0; buffer.total()];

        // SAFETY: `tty` points to a valid `TTY` instance.
        let nread = unsafe {
            tty.read(
                c_buffer.as_mut_ptr() as *mut _,
                c_buffer.len(),
                c_buffer.len(),
            )
        };

        match nread {
            n if n < 0 => Err((-n) as u32),
            0 => Ok(0),
            n => Ok(buffer.fill(&c_buffer[..n as usize])?.allow_partial()),
        }
    }

    fn write(&self, buffer: &[u8]) -> KResult<usize> {
        // SAFETY: `tty` should always valid.
        let tty = unsafe { self.tty.as_ptr().as_mut().unwrap() };

        for &ch in buffer.iter() {
            // SAFETY: `tty` points to a valid `TTY` instance.
            unsafe { tty.show_char(ch as i32) };
        }

        Ok(buffer.len())
    }

    fn ioctl(&self, request: usize, arg3: usize) -> KResult<usize> {
        // SAFETY: `tty` should always valid.
        let tty = unsafe { self.tty.as_ptr().as_mut().unwrap() };

        // SAFETY: `tty` points to a valid `TTY` instance.
        let result = unsafe { tty.ioctl(request as c_int, arg3 as c_ulong) };

        match result {
            0 => Ok(0),
            _ => Err((-result) as u32),
        }
    }
}

impl File {
    pub fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        match self {
            File::Inode(inode) => inode.read(buffer),
            File::PipeRead(pipe) => pipe.pipe.read(buffer),
            File::TTY(tty) => tty.read(buffer),
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

    pub fn write(&self, buffer: &[u8]) -> KResult<usize> {
        match self {
            File::Inode(inode) => inode.write(buffer),
            File::PipeWrite(pipe) => pipe.pipe.write(buffer),
            File::TTY(tty) => tty.write(buffer),
            _ => Err(EBADF),
        }
    }

    pub fn seek(&self, option: SeekOption) -> KResult<usize> {
        match self {
            File::Inode(inode) => inode.seek(option),
            File::PipeRead(_) | File::PipeWrite(_) | File::TTY(_) => Err(ESPIPE),
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

    pub fn sendfile(&self, dest_file: &Self, count: usize) -> KResult<usize> {
        let buffer_page = Page::alloc_one();

        match self {
            File::Inode(file) if s_isblk(file.mode) || s_isreg(file.mode) => (),
            _ => return Err(EINVAL),
        }

        // TODO!!!: zero copy implementation with mmap
        let mut tot = 0usize;
        while tot < count {
            if has_pending_signal() {
                if tot == 0 {
                    return Err(EINTR);
                } else {
                    return Ok(tot);
                }
            }

            let batch_size = usize::min(count - tot, buffer_page.len());
            let slice = buffer_page.as_cached().as_mut_slice::<u8>(batch_size);
            let mut buffer = RawBuffer::new_from_slice(slice);

            let nwrote = self.read(&mut buffer)?;

            if nwrote == 0 {
                break;
            }

            tot += dest_file.write(&slice[..nwrote])?;
        }

        Ok(tot)
    }

    pub fn ioctl(&self, request: usize, arg3: usize) -> KResult<usize> {
        match self {
            File::TTY(tty) => tty.ioctl(request, arg3),
            _ => Err(ENOTTY),
        }
    }
}
