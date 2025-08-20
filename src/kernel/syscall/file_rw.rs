use super::{FromSyscallArg, User};
use crate::io::{ByteBuffer, IntoStream};
use crate::kernel::constants::{
    EBADF, EFAULT, EINVAL, ENOENT, ENOTDIR, SEEK_CUR, SEEK_END, SEEK_SET,
};
use crate::kernel::syscall::UserMut;
use crate::kernel::task::Thread;
use crate::kernel::timer::sleep;
use crate::kernel::vfs::filearray::FD;
use crate::kernel::vfs::inode::Mode;
use crate::kernel::vfs::EventFile;
use crate::kernel::vfs::File;
use crate::kernel::vfs::{PollEvent, SeekOption};
use crate::{
    io::{Buffer, BufferFill},
    kernel::{
        user::{CheckedUserPointer, UserBuffer, UserPointer, UserPointerMut, UserString},
        vfs::dentry::Dentry,
    },
    path::Path,
    prelude::*,
};
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::time::Duration;
use posix_types::ctypes::{Long, PtrT};
use posix_types::namei::RenameFlags;
use posix_types::open::{AtFlags, OpenFlags};
use posix_types::poll::FDSet;
use posix_types::signal::SigSet;
use posix_types::stat::Stat;
use posix_types::stat::{StatX, TimeSpec};
use posix_types::syscall_no::*;

impl FromSyscallArg for OpenFlags {
    fn from_arg(value: usize) -> Self {
        OpenFlags::from_bits_retain(value as u32)
    }
}

impl FromSyscallArg for AtFlags {
    fn from_arg(value: usize) -> Self {
        AtFlags::from_bits_retain(value as u32)
    }
}

fn dentry_from(
    thread: &Thread,
    dirfd: FD,
    pathname: User<u8>,
    follow_symlink: bool,
) -> KResult<Arc<Dentry>> {
    let path = UserString::new(pathname)?;

    match (path.as_cstr().to_bytes_with_nul()[0], dirfd) {
        (b'/', _) | (_, FD::AT_FDCWD) => {
            let path = Path::new(path.as_cstr().to_bytes())?;
            Dentry::open(&thread.fs_context, path, follow_symlink)
        }
        (0, dirfd) => {
            let dir_file = thread.files.get(dirfd).ok_or(EBADF)?;
            dir_file.as_path().ok_or(EBADF).cloned()
        }
        (_, dirfd) => {
            let path = Path::new(path.as_cstr().to_bytes())?;
            let dir_file = thread.files.get(dirfd).ok_or(EBADF)?;
            let dir_dentry = dir_file.as_path().ok_or(ENOTDIR)?;

            Dentry::open_at(&thread.fs_context, dir_dentry, path, follow_symlink)
        }
    }
}

#[eonix_macros::define_syscall(SYS_READ)]
async fn read(fd: FD, buffer: UserMut<u8>, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    thread
        .files
        .get(fd)
        .ok_or(EBADF)?
        .read(&mut buffer, None)
        .await
}

#[eonix_macros::define_syscall(SYS_PREAD64)]
async fn pread64(fd: FD, buffer: UserMut<u8>, bufsize: usize, offset: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    thread
        .files
        .get(fd)
        .ok_or(EBADF)?
        .read(&mut buffer, Some(offset))
        .await
}

#[eonix_macros::define_syscall(SYS_WRITE)]
async fn write(fd: FD, buffer: User<u8>, count: usize) -> KResult<usize> {
    let buffer = CheckedUserPointer::new(buffer, count)?;
    let mut stream = buffer.into_stream();

    thread
        .files
        .get(fd)
        .ok_or(EBADF)?
        .write(&mut stream, None)
        .await
}

#[eonix_macros::define_syscall(SYS_PWRITE64)]
async fn pwrite64(fd: FD, buffer: User<u8>, count: usize, offset: usize) -> KResult<usize> {
    let buffer = CheckedUserPointer::new(buffer, count)?;
    let mut stream = buffer.into_stream();

    thread
        .files
        .get(fd)
        .ok_or(EBADF)?
        .write(&mut stream, Some(offset))
        .await
}

#[eonix_macros::define_syscall(SYS_OPENAT)]
async fn openat(dirfd: FD, pathname: User<u8>, flags: OpenFlags, mut mode: Mode) -> KResult<FD> {
    let dentry = dentry_from(thread, dirfd, pathname, flags.follow_symlink())?;

    let umask = *thread.fs_context.umask.lock();
    mode.mask_perm(!umask.non_format_bits());

    thread.files.open(&dentry, flags, mode)
}

#[eonix_macros::define_syscall(SYS_FTRUNCATE64)]
async fn ftruncate(fd: FD, new_size: usize) -> KResult<()> {
    thread.files.get(fd).ok_or(EBADF)?.truncate(new_size)
}

async fn do_copy_or_splice(
    file_in: File,
    off_in: isize,
    file_out: File,
    off_out: isize,
    len: usize,
    is_splice: bool,
) -> KResult<isize> {
    let off_in_ptr = off_in as *mut isize;
    let off_out = off_out as *mut isize;
    let (input_offset, input_buffer) = match off_in_ptr.is_null() {
        true => (None, None),
        false => {
            let buffer = UserBuffer::new(UserMut::with_addr(off_in as usize), size_of::<isize>())?;
            let offset = isize::from_le_bytes(buffer.as_slice().try_into().unwrap());
            if offset > file_in.size().try_into().unwrap() {
                return Ok(0);
            }
            if offset < 0 {
                return Ok(-1);
            }
            (Some(offset), Some(buffer))
        }
    };

    let (output_offset, output_buffer) = match off_out.is_null() {
        true => (None, None),
        false => {
            let buffer = UserBuffer::new(UserMut::with_addr(off_out as usize), size_of::<isize>())?;
            let offset = isize::from_le_bytes(buffer.as_slice().try_into().unwrap());
            if offset < 0 {
                return Ok(-1);
            }
            (Some(offset), Some(buffer))
        }
    };

    let mut total_copied = 0usize;
    let mut remaining = len;
    let buffer_size = 4096;
    let mut buffer = vec![0u8; buffer_size];

    while remaining > 0 {
        let to_read = core::cmp::min(remaining, buffer_size);
        if to_read == 0 {
            break;
        }

        let mut byte_buffer = ByteBuffer::new(&mut buffer[..to_read]);
        let read_bytes = match input_offset {
            Some(offset) => {
                file_in
                    .read(&mut byte_buffer, Some(offset as usize + total_copied))
                    .await?
            }
            None => file_in.read(&mut byte_buffer, None).await?,
        };
        if read_bytes == 0 {
            break;
        }

        let mut stream = (&buffer[..read_bytes]).into_stream();
        let written_bytes = match output_offset {
            Some(offset) => {
                file_out
                    .write(&mut stream, Some(offset as usize + total_copied))
                    .await?
            }
            None => file_out.write(&mut stream, None).await?,
        };
        if written_bytes == 0 {
            break;
        }

        total_copied += written_bytes;
        remaining -= written_bytes;

        if written_bytes < read_bytes {
            return Ok(-1);
        }

        if is_splice && input_offset.is_none() {
            break;
        }
    }

    match (input_offset, input_buffer) {
        (Some(offset), Some(mut buffer)) => {
            let _ = buffer.fill(&(offset + total_copied as isize).to_le_bytes());
        }
        _ => (),
    }

    match (output_offset, output_buffer) {
        (Some(offset), Some(mut buffer)) => {
            let _ = buffer.fill(&(offset + total_copied as isize).to_le_bytes());
        }
        _ => (),
    }

    Ok(total_copied as isize)
}

#[eonix_macros::define_syscall(SYS_COPY_FILE_RANGE)]
async fn copy_file_range(
    fd_in: FD,
    off_in: isize,
    fd_out: FD,
    off_out: isize,
    len: usize,
    _flags: u32,
) -> KResult<isize> {
    if len == 0 {
        return Ok(0);
    }

    let file_in = thread.files.get(fd_in).ok_or(EBADF)?.clone();
    let file_out = thread.files.get(fd_out).ok_or(EBADF)?.clone();

    do_copy_or_splice(file_in, off_in, file_out, off_out, len, false).await
}

#[eonix_macros::define_syscall(SYS_SPLICE)]
async fn splice(
    fd_in: FD,
    off_in: isize,
    fd_out: FD,
    off_out: isize,
    len: usize,
    _flags: u32,
) -> KResult<isize> {
    if len == 0 {
        return Ok(0);
    }

    let file_in = thread.files.get(fd_in).ok_or(EBADF)?.clone();
    let file_out = thread.files.get(fd_out).ok_or(EBADF)?.clone();

    do_copy_or_splice(file_in, off_in, file_out, off_out, len, true).await
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_OPEN)]
async fn open(path: User<u8>, flags: OpenFlags, mode: u32) -> KResult<FD> {
    sys_openat(thread, FD::AT_FDCWD, path, flags, mode).await
}

#[eonix_macros::define_syscall(SYS_CLOSE)]
async fn close(fd: FD) -> KResult<()> {
    thread.files.close(fd).await
}

#[eonix_macros::define_syscall(SYS_DUP)]
async fn dup(fd: FD) -> KResult<FD> {
    thread.files.dup(fd)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_DUP2)]
async fn dup2(old_fd: FD, new_fd: FD) -> KResult<FD> {
    thread.files.dup_to(old_fd, new_fd, OpenFlags::empty())
}

#[eonix_macros::define_syscall(SYS_DUP3)]
async fn dup3(old_fd: FD, new_fd: FD, flags: OpenFlags) -> KResult<FD> {
    thread.files.dup_to(old_fd, new_fd, flags).await
}

#[eonix_macros::define_syscall(SYS_PIPE2)]
async fn pipe2(pipe_fd: UserMut<[FD; 2]>, flags: OpenFlags) -> KResult<()> {
    let mut buffer = UserBuffer::new(pipe_fd.cast(), core::mem::size_of::<[FD; 2]>())?;
    let (read_fd, write_fd) = thread.files.pipe(flags)?;

    buffer.copy(&[read_fd, write_fd])?.ok_or(EFAULT)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_PIPE)]
async fn pipe(pipe_fd: UserMut<[FD; 2]>) -> KResult<()> {
    sys_pipe2(thread, pipe_fd, OpenFlags::empty()).await
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_GETDENTS)]
async fn getdents(fd: FD, buffer: UserMut<u8>, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    thread.files.get(fd).ok_or(EBADF)?.getdents(&mut buffer)?;
    Ok(buffer.wrote())
}

#[eonix_macros::define_syscall(SYS_GETDENTS64)]
async fn getdents64(fd: FD, buffer: UserMut<u8>, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    thread
        .files
        .get(fd)
        .ok_or(EBADF)?
        .getdents64(&mut buffer)
        .await?;
    Ok(buffer.wrote())
}

#[cfg_attr(
    not(target_arch = "x86_64"),
    eonix_macros::define_syscall(SYS_NEWFSTATAT)
)]
#[cfg_attr(target_arch = "x86_64", eonix_macros::define_syscall(SYS_FSTATAT64))]
async fn newfstatat(
    dirfd: FD,
    pathname: User<u8>,
    statbuf: UserMut<Stat>,
    flags: AtFlags,
) -> KResult<()> {
    let dentry = if flags.at_empty_path() {
        let file = thread.files.get(dirfd).ok_or(EBADF)?;
        file.as_path().ok_or(EBADF)?.clone()
    } else {
        dentry_from(thread, dirfd, pathname, !flags.no_follow())?
    };

    let statbuf = UserPointerMut::new(statbuf)?;

    let mut statx = StatX::default();
    dentry.statx(&mut statx, u32::MAX)?;

    statbuf.write(statx.into())?;

    Ok(())
}

#[cfg_attr(
    not(target_arch = "x86_64"),
    eonix_macros::define_syscall(SYS_NEWFSTAT)
)]
#[cfg_attr(target_arch = "x86_64", eonix_macros::define_syscall(SYS_FSTAT64))]
async fn newfstat(fd: FD, statbuf: UserMut<Stat>) -> KResult<()> {
    sys_newfstatat(thread, fd, User::null(), statbuf, AtFlags::AT_EMPTY_PATH).await
}

#[eonix_macros::define_syscall(SYS_STATX)]
async fn statx(
    dirfd: FD,
    pathname: User<u8>,
    flags: AtFlags,
    mask: u32,
    buffer: UserMut<StatX>,
) -> KResult<()> {
    if !flags.statx_default_sync() {
        unimplemented!("statx with no default sync flags: {:?}", flags);
    }

    let mut statx = StatX::default();
    let buffer = UserPointerMut::new(buffer)?;

    let dentry = if flags.at_empty_path() {
        let file = thread.files.get(dirfd).ok_or(EBADF)?;
        file.as_path().ok_or(EBADF)?.clone()
    } else {
        dentry_from(thread, dirfd, pathname, !flags.no_follow())?
    };

    dentry.statx(&mut statx, mask)?;
    buffer.write(statx)?;

    Ok(())
}

#[eonix_macros::define_syscall(SYS_MKDIRAT)]
async fn mkdirat(dirfd: FD, pathname: User<u8>, mut mode: Mode) -> KResult<()> {
    let umask = *thread.fs_context.umask.lock();
    mode.mask_perm(!umask.non_format_bits());

    let dentry = dentry_from(thread, dirfd, pathname, true)?;
    dentry.mkdir(mode)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_MKDIR)]
async fn mkdir(pathname: User<u8>, mode: u32) -> KResult<()> {
    sys_mkdirat(thread, FD::AT_FDCWD, pathname, mode).await
}

#[eonix_macros::define_syscall(SYS_FTRUNCATE64)]
async fn truncate64(fd: FD, length: usize) -> KResult<()> {
    let file = thread.files.get(fd).ok_or(EBADF)?;
    file.as_path().ok_or(EBADF)?.truncate(length)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_TRUNCATE)]
async fn truncate(pathname: User<u8>, length: usize) -> KResult<()> {
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&thread.fs_context, path, true)?;

    dentry.truncate(length)
}

#[eonix_macros::define_syscall(SYS_UNLINKAT)]
async fn unlinkat(dirfd: FD, pathname: User<u8>) -> KResult<()> {
    dentry_from(thread, dirfd, pathname, false)?.unlink()
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_UNLINK)]
async fn unlink(pathname: User<u8>) -> KResult<()> {
    sys_unlinkat(thread, FD::AT_FDCWD, pathname)
}

#[eonix_macros::define_syscall(SYS_SYMLINKAT)]
async fn symlinkat(target: User<u8>, dirfd: FD, linkpath: User<u8>) -> KResult<()> {
    let target = UserString::new(target)?;
    let dentry = dentry_from(thread, dirfd, linkpath, false)?;

    dentry.symlink(target.as_cstr().to_bytes())
}

#[eonix_macros::define_syscall(SYS_LINKAT)]
async fn linkat(
    old_dirfd: FD,
    old_pathname: User<u8>,
    new_dirfd: FD,
    new_pathname: User<u8>,
    _flags: i32,
) -> KResult<()> {
    let old_dentry = dentry_from(thread, old_dirfd, old_pathname, false)?;
    let new_dentry = dentry_from(thread, new_dirfd, new_pathname, false)?;
    let old_inode = old_dentry.get_inode().unwrap();

    new_dentry.linkat(old_inode)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_SYMLINK)]
async fn symlink(target: User<u8>, linkpath: User<u8>) -> KResult<()> {
    sys_symlinkat(thread, target, FD::AT_FDCWD, linkpath)
}

#[eonix_macros::define_syscall(SYS_MKNODAT)]
async fn mknodat(dirfd: FD, pathname: User<u8>, mut mode: Mode, dev: u32) -> KResult<()> {
    if !mode.is_blk() && !mode.is_chr() {
        return Err(EINVAL);
    }

    let dentry = dentry_from(thread, dirfd, pathname, true)?;

    let umask = *thread.fs_context.umask.lock();
    mode.mask_perm(!umask.non_format_bits());

    dentry.mknod(mode, dev)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_MKNOD)]
async fn mknod(pathname: User<u8>, mode: u32, dev: u32) -> KResult<()> {
    sys_mknodat(thread, FD::AT_FDCWD, pathname, mode, dev).await
}

#[eonix_macros::define_syscall(SYS_READLINKAT)]
async fn readlinkat(
    dirfd: FD,
    pathname: User<u8>,
    buffer: UserMut<u8>,
    bufsize: usize,
) -> KResult<usize> {
    let dentry = dentry_from(thread, dirfd, pathname, false)?;
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    dentry.readlink(&mut buffer)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_READLINK)]
async fn readlink(pathname: User<u8>, buffer: UserMut<u8>, bufsize: usize) -> KResult<usize> {
    sys_readlinkat(thread, FD::AT_FDCWD, pathname, buffer, bufsize).await
}

async fn do_lseek(thread: &Thread, fd: FD, offset: u64, whence: u32) -> KResult<u64> {
    let file = thread.files.get(fd).ok_or(EBADF)?;

    Ok(match whence {
        SEEK_SET => file.seek(SeekOption::Set(offset as usize)).await?,
        SEEK_CUR => file.seek(SeekOption::Current(offset as isize)).await?,
        SEEK_END => file.seek(SeekOption::End(offset as isize)).await?,
        _ => return Err(EINVAL),
    } as u64)
}

#[cfg(not(target_arch = "x86_64"))]
#[eonix_macros::define_syscall(SYS_LSEEK)]
async fn lseek(fd: FD, offset: u64, whence: u32) -> KResult<u64> {
    do_lseek(thread, fd, offset, whence).await
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_LLSEEK)]
fn llseek(
    fd: FD,
    offset_high: u32,
    offset_low: u32,
    result: UserMut<u64>,
    whence: u32,
) -> KResult<()> {
    let mut result = UserBuffer::new(result.cast(), core::mem::size_of::<u64>())?;
    let offset = ((offset_high as u64) << 32) | (offset_low as u64);

    let new_offset = do_lseek(thread, fd, offset, whence).await?;

    result.copy(&new_offset)?.ok_or(EFAULT)
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct IoVec {
    pub base: PtrT,
    pub len: Long,
}

#[eonix_macros::define_syscall(SYS_READV)]
async fn readv(fd: FD, iov_user: User<IoVec>, iovcnt: u32) -> KResult<usize> {
    let file = thread.files.get(fd).ok_or(EBADF)?;

    let mut iov_user = UserPointer::new(iov_user)?;
    let iov_buffers = (0..iovcnt)
        .map(|_| {
            let iov_result = iov_user.read()?;
            iov_user = iov_user.offset(1)?;
            Ok(iov_result)
        })
        .filter_map(|iov_result| match iov_result {
            Err(err) => Some(Err(err)),
            Ok(IoVec {
                len: Long::ZERO, ..
            }) => None,
            Ok(IoVec { base, len }) => {
                Some(UserBuffer::new(UserMut::with_addr(base.addr()), len.get()))
            }
        })
        .collect::<KResult<Vec<_>>>()?;

    let mut tot = 0usize;
    for mut buffer in iov_buffers.into_iter() {
        // TODO!!!: `readv`
        let nread = file.read(&mut buffer, None).await?;
        tot += nread;

        if nread != buffer.total() {
            break;
        }
    }

    Ok(tot)
}

#[eonix_macros::define_syscall(SYS_WRITEV)]
async fn writev(fd: FD, iov_user: User<IoVec>, iovcnt: u32) -> KResult<usize> {
    let file = thread.files.get(fd).ok_or(EBADF)?;

    let mut iov_user = UserPointer::new(iov_user)?;
    let iov_streams = (0..iovcnt)
        .map(|_| {
            let iov_result = iov_user.read()?;
            iov_user = iov_user.offset(1)?;
            Ok(iov_result)
        })
        .filter_map(|iov_result| match iov_result {
            Err(err) => Some(Err(err)),
            Ok(IoVec {
                len: Long::ZERO, ..
            }) => None,
            Ok(IoVec { base, len }) => Some(
                CheckedUserPointer::new(User::with_addr(base.addr()), len.get())
                    .map(|ptr| ptr.into_stream()),
            ),
        })
        .collect::<KResult<Vec<_>>>()?;

    let mut tot = 0usize;
    for mut stream in iov_streams.into_iter() {
        let nread = file.write(&mut stream, None).await?;
        tot += nread;

        if nread == 0 || !stream.is_drained() {
            break;
        }
    }

    Ok(tot)
}

#[eonix_macros::define_syscall(SYS_FACCESSAT)]
async fn faccessat(dirfd: FD, pathname: User<u8>, _mode: u32, flags: AtFlags) -> KResult<()> {
    let dentry = dentry_from(thread, dirfd, pathname, !flags.no_follow())?;

    if !dentry.is_valid() {
        return Err(ENOENT);
    }

    // TODO: check permission
    // match mode {
    //     F_OK => todo!(),
    //     R_OK => todo!(),
    //     W_OK => todo!(),
    //     X_OK => todo!(),
    //     _ => Err(EINVAL),
    // }

    Ok(())
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_ACCESS)]
async fn access(pathname: User<u8>, mode: u32) -> KResult<()> {
    sys_faccessat(thread, FD::AT_FDCWD, pathname, mode, AtFlags::empty()).await
}

#[eonix_macros::define_syscall(SYS_SENDFILE64)]
async fn sendfile64(out_fd: FD, in_fd: FD, offset: UserMut<u8>, count: usize) -> KResult<usize> {
    let in_file = thread.files.get(in_fd).ok_or(EBADF)?;
    let out_file = thread.files.get(out_fd).ok_or(EBADF)?;

    if !offset.is_null() {
        unimplemented!("sendfile64 with offset");
    }

    in_file.sendfile(&out_file, count).await
}

#[eonix_macros::define_syscall(SYS_IOCTL)]
async fn ioctl(fd: FD, request: usize, arg3: usize) -> KResult<usize> {
    let file = thread.files.get(fd).ok_or(EBADF)?;

    file.ioctl(request, arg3).await
}

#[eonix_macros::define_syscall(SYS_FCNTL64)]
async fn fcntl64(fd: FD, cmd: u32, arg: usize) -> KResult<usize> {
    thread.files.fcntl(fd, cmd, arg)
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct UserPollFd {
    fd: FD,
    events: u16,
    revents: u16,
}

pub struct PPollFuture {
    polls: Vec<(File, PollEvent)>,
}

impl Future for PPollFuture {
    type Output = KResult<Option<Vec<(usize, PollEvent)>>>;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let mut poll_res = Vec::new();
        for (i, (file, event)) in self.polls.iter().enumerate() {
            let result = unsafe { Pin::new_unchecked(&mut file.poll(*event)).poll(cx) };

            match result {
                core::task::Poll::Ready(Ok(revent)) => {
                    poll_res.push((i, revent));
                }
                core::task::Poll::Ready(Err(err)) => return core::task::Poll::Ready(Err(err)),
                core::task::Poll::Pending => {}
            }
        }

        if !poll_res.is_empty() {
            core::task::Poll::Ready(Ok(Some(poll_res)))
        } else {
            core::task::Poll::Ready(Ok(None))
        }
    }
}

async fn do_poll(
    thread: &Thread,
    fds: UserMut<UserPollFd>,
    nfds: u32,
    _timeout: u32,
) -> KResult<u32> {
    match nfds {
        0 => Ok(0),
        2.. => {
            let mut polls = Vec::with_capacity(nfds as usize);
            for i in 0..nfds {
                let fd = UserPointerMut::new(fds)?
                    .offset(i as isize)
                    .map_err(|_| EFAULT)?
                    .read()?;
                let file = thread.files.get(fd.fd).ok_or(EBADF)?;
                polls.push((file, PollEvent::from_bits_retain(fd.events)));
            }

            loop {
                let polls_future = PPollFuture {
                    polls: polls.clone(),
                };
                let res = polls_future.await?;

                if res.is_some() {
                    let fds_mut = UserPointerMut::new(fds)?;
                    let len = res.as_ref().unwrap().len();
                    for (i, revent) in res.unwrap() {
                        let mut fd = fds_mut.offset(i as isize).map_err(|_| EFAULT)?.read()?;
                        fd.revents = revent.bits();
                        fds_mut.offset(i as isize)?.write(fd)?;
                    }
                    return Ok(len as u32);
                }
                sleep(Duration::from_millis(100)).await
            }
        }
        1 => {
            // TODO!!: Poll with timeout
            // if timeout != u32::MAX {
            //     unimplemented!("Poll with timeout {}", timeout);
            // }
            let fds = UserPointerMut::new(fds)?;
            let mut fd = fds.read()?;

            let file = thread.files.get(fd.fd).ok_or(EBADF)?;
            fd.revents = file
                .poll(PollEvent::from_bits_retain(fd.events))
                .await?
                .bits();

            fds.write(fd)?;
            Ok(1)
        }
    }
}

#[eonix_macros::define_syscall(SYS_PPOLL)]
async fn ppoll(
    fds: UserMut<UserPollFd>,
    nfds: u32,
    _timeout_ptr: User<TimeSpec>,
    _sigmask: User<SigSet>,
) -> KResult<u32> {
    // TODO: Implement ppoll with signal mask and timeout
    do_poll(thread, fds, nfds, 0).await
}

#[eonix_macros::define_syscall(SYS_PSELECT6)]
async fn pselect6(
    nfds: u32,
    readfds: UserMut<FDSet>,
    writefds: UserMut<FDSet>,
    _exceptfds: UserMut<FDSet>,
    timeout: UserMut<TimeSpec>,
    _sigmask: User<()>,
) -> KResult<usize> {
    // According to [pthread6(2)](https://linux.die.net/man/2/pselect6):
    // Some code calls select() with all three sets empty, nfds zero, and
    // a non-NULL timeout as a fairly portable way to sleep with subsecond precision.
    if nfds == 0 {
        let timeout = UserPointerMut::new(timeout)?;

        // Read here to check for invalid pointers.
        let _timeout_value = timeout.read()?;

        sleep(Duration::from_millis(10)).await;

        timeout.write(TimeSpec {
            tv_sec: 0,
            tv_nsec: 0,
        })?;

        return Ok(0);
    }

    let time_out = if timeout.is_null() {
        None
    } else {
        let timeout = UserPointer::new(timeout.as_const())?.read()?;
        if timeout.tv_nsec == 0 && timeout.tv_sec == 0 {
            return Ok(0);
        }

        Some(timeout)
    };

    let mut read_fds = if readfds.is_null() {
        None
    } else {
        Some(UserPointer::new(readfds.as_const())?.read()?)
    };
    let mut write_fds = if writefds.is_null() {
        None
    } else {
        Some(UserPointer::new(writefds.as_const())?.read()?)
    };

    let poll_fds = {
        let mut poll_fds = Vec::with_capacity(nfds as usize);
        for fd in 0..nfds {
            let events = {
                let readable = read_fds.as_ref().is_some_and(|fds| fds.is_set(fd));
                let writable = write_fds.as_ref().is_some_and(|fds| fds.is_set(fd));
                let mut events = PollEvent::empty();

                if readable {
                    events |= PollEvent::Readable;
                }
                if writable {
                    events |= PollEvent::Writable;
                }
                events
            };

            if !events.is_empty() {
                poll_fds.push((fd, events));
            }
        }
        poll_fds
    };

    if let Some(fds) = &mut read_fds {
        fds.clear();
    }
    if let Some(fds) = &mut write_fds {
        fds.clear();
    }

    let mut tot = 0;
    let mut time_cnt = time_out.map(|time| (time.tv_sec) / 50);

    if poll_fds.len() > 1 {
        unimplemented!("Poll with multiple fds: {:?}", poll_fds);
    }

    loop {
        for (fd, events) in &poll_fds {
            let res = thread
                .files
                .get(FD::from(*fd))
                .ok_or(EBADF)?
                .poll(events.clone())
                .await?;

            if res.contains(PollEvent::Readable) {
                if let Some(fds) = &mut read_fds {
                    fds.set(*fd);
                    tot += 1;
                }
            }
            if res.contains(PollEvent::Writable) {
                if let Some(fds) = &mut write_fds {
                    fds.set(*fd);
                    tot += 1;
                }
            }
        }

        if tot > 0 {
            break;
        }

        // Since we already have a background iface poll task, simply sleep for a while
        sleep(Duration::from_millis(50)).await;

        if let Some(cnt) = &mut time_cnt {
            if *cnt == 0 {
                break;
            }
            *cnt = *cnt - 1;
        }
    }

    if let Some(fds) = read_fds {
        UserPointerMut::new(readfds)?.write(fds)?;
    }
    if let Some(fds) = write_fds {
        UserPointerMut::new(writefds)?.write(fds)?;
    }

    Ok(tot)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_POLL)]
async fn poll(fds: UserMut<UserPollFd>, nfds: u32, timeout: u32) -> KResult<u32> {
    do_poll(thread, fds, nfds, timeout).await
}

#[eonix_macros::define_syscall(SYS_FCHOWNAT)]
async fn fchownat(
    dirfd: FD,
    pathname: User<u8>,
    uid: u32,
    gid: u32,
    flags: AtFlags,
) -> KResult<()> {
    let dentry = dentry_from(thread, dirfd, pathname, !flags.no_follow())?;
    if !dentry.is_valid() {
        return Err(ENOENT);
    }

    dentry.chown(uid, gid)
}

#[eonix_macros::define_syscall(SYS_FCHMODAT)]
async fn fchmodat(dirfd: FD, pathname: User<u8>, mode: Mode) -> KResult<()> {
    let dentry = dentry_from(thread, dirfd, pathname, true)?;

    if !dentry.is_valid() {
        return Err(ENOENT);
    }

    dentry.chmod(mode)
}

#[eonix_macros::define_syscall(SYS_FCHMOD)]
async fn chmod(pathname: User<u8>, mode: Mode) -> KResult<()> {
    sys_fchmodat(thread, FD::AT_FDCWD, pathname, mode).await
}

#[eonix_macros::define_syscall(SYS_UTIMENSAT)]
async fn utimensat(
    dirfd: FD,
    pathname: User<u8>,
    times: User<TimeSpec>,
    flags: AtFlags,
) -> KResult<()> {
    let dentry = if flags.at_empty_path() {
        let file = thread.files.get(dirfd).ok_or(EBADF)?;
        file.as_path().ok_or(EBADF)?.clone()
    } else {
        dentry_from(thread, dirfd, pathname, !flags.no_follow())?
    };

    if !dentry.is_valid() {
        return Err(ENOENT);
    }

    let _times = if times.is_null() {
        [TimeSpec::default(), TimeSpec::default()]
    } else {
        let times = UserPointer::new(times)?;
        [times.read()?, times.offset(1)?.read()?]
    };

    // TODO: Implement utimensat
    // dentry.utimens(&times)
    Ok(())
}

#[eonix_macros::define_syscall(SYS_RENAMEAT2)]
async fn renameat2(
    old_dirfd: FD,
    old_pathname: User<u8>,
    new_dirfd: FD,
    new_pathname: User<u8>,
    flags: u32,
) -> KResult<()> {
    let flags = RenameFlags::from_bits(flags).ok_or(EINVAL)?;

    // The two flags RENAME_NOREPLACE and RENAME_EXCHANGE are mutually exclusive.
    if flags.contains(RenameFlags::RENAME_NOREPLACE | RenameFlags::RENAME_EXCHANGE) {
        Err(EINVAL)?;
    }

    let old_dentry = dentry_from(thread, old_dirfd, old_pathname, false)?;
    let new_dentry = dentry_from(thread, new_dirfd, new_pathname, false)?;

    old_dentry.rename(&new_dentry, flags)
}

#[eonix_macros::define_syscall(SYS_MSYNC)]
async fn msync(/* fill the actual args here */) {
    // TODO
}

#[eonix_macros::define_syscall(SYS_FALLOCATE)]
async fn falllocate(/* fill the actual args here */) -> KResult<()> {
    Ok(())
}

#[eonix_macros::define_syscall(SYS_EVENTFD2)]
async fn eventfd2(init_val: u64, flags: u32) -> KResult<FD> {
    const IS_NONBLOCK: u32 = 1 << 11;

    thread
        .files
        .event_file(EventFile::new(init_val, flags & IS_NONBLOCK != 0))
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_RENAME)]
async fn rename(old_pathname: User<u8>, new_pathname: User<u8>) -> KResult<()> {
    sys_renameat2(
        thread,
        FD::AT_FDCWD,
        old_pathname,
        FD::AT_FDCWD,
        new_pathname,
        0,
    )
    .await
}

#[eonix_macros::define_syscall(SYS_LISTXATTR)]
async fn listxattr() {}

pub fn keep_alive() {}
