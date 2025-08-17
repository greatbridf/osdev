use core::time::{self, Duration};

use super::FromSyscallArg;
use crate::io::IntoStream;
use crate::kernel::constants::{
    EBADF, EFAULT, EINVAL, ENOENT, ENOSYS, ENOTDIR, SEEK_CUR, SEEK_END, SEEK_SET, S_IFBLK, S_IFCHR,
};
use crate::kernel::task::Thread;
use crate::kernel::timer::sleep;
use crate::kernel::vfs::filearray::FD;
use crate::{
    io::{Buffer, BufferFill},
    kernel::{
        user::{
            dataflow::{CheckedUserPointer, UserBuffer, UserString},
            UserPointer, UserPointerMut,
        },
        vfs::{
            dentry::Dentry,
            file::{PollEvent, SeekOption},
        },
    },
    path::Path,
    prelude::*,
};
use alloc::slice;
use alloc::sync::Arc;
use bitflags::bitflags;
use eonix_runtime::task::Task;
use posix_types::ctypes::{Long, PtrT};
use posix_types::namei::RenameFlags;
use posix_types::open::{AtFlags, OpenFlags};
use posix_types::poll::FDSet;
use posix_types::signal::{SigSet, Signal};
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
    pathname: *const u8,
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
fn read(fd: FD, buffer_: *mut u8, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer_, bufsize)?;

    Task::block_on(thread.files.get(fd).ok_or(EBADF)?.read(&mut buffer, None))
}

#[eonix_macros::define_syscall(SYS_PREAD64)]
fn pread64(fd: FD, buffer: *mut u8, bufsize: usize, offset: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    Task::block_on(
        thread
            .files
            .get(fd)
            .ok_or(EBADF)?
            .read(&mut buffer, Some(offset)),
    )
}

#[eonix_macros::define_syscall(SYS_WRITE)]
fn write(fd: FD, buffer: *const u8, count: usize) -> KResult<usize> {
    let buffer = CheckedUserPointer::new(buffer, count)?;
    let mut stream = buffer.into_stream();

    Task::block_on(thread.files.get(fd).ok_or(EBADF)?.write(&mut stream, None))
}

#[eonix_macros::define_syscall(SYS_PWRITE64)]
fn pwrite64(fd: FD, buffer: *const u8, count: usize, offset: usize) -> KResult<usize> {
    let buffer = CheckedUserPointer::new(buffer, count)?;
    let mut stream = buffer.into_stream();

    Task::block_on(
        thread
            .files
            .get(fd)
            .ok_or(EBADF)?
            .write(&mut stream, Some(offset)),
    )
}

#[eonix_macros::define_syscall(SYS_OPENAT)]
fn openat(dirfd: FD, pathname: *const u8, flags: OpenFlags, mode: u32) -> KResult<FD> {
    let dentry = dentry_from(thread, dirfd, pathname, flags.follow_symlink())?;
    thread.files.open(&dentry, flags, mode)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_OPEN)]
fn open(path: *const u8, flags: OpenFlags, mode: u32) -> KResult<FD> {
    sys_openat(thread, FD::AT_FDCWD, path, flags, mode)
}

#[eonix_macros::define_syscall(SYS_CLOSE)]
fn close(fd: FD) -> KResult<()> {
    thread.files.close(fd)
}

#[eonix_macros::define_syscall(SYS_DUP)]
fn dup(fd: FD) -> KResult<FD> {
    thread.files.dup(fd)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_DUP2)]
fn dup2(old_fd: FD, new_fd: FD) -> KResult<FD> {
    thread.files.dup_to(old_fd, new_fd, OpenFlags::empty())
}

#[eonix_macros::define_syscall(SYS_DUP3)]
fn dup3(old_fd: FD, new_fd: FD, flags: OpenFlags) -> KResult<FD> {
    thread.files.dup_to(old_fd, new_fd, flags)
}

#[eonix_macros::define_syscall(SYS_PIPE2)]
fn pipe2(pipe_fd: *mut [FD; 2], flags: OpenFlags) -> KResult<()> {
    let mut buffer = UserBuffer::new(pipe_fd as *mut u8, core::mem::size_of::<[FD; 2]>())?;
    let (read_fd, write_fd) = thread.files.pipe(flags)?;

    buffer.copy(&[read_fd, write_fd])?.ok_or(EFAULT)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_PIPE)]
fn pipe(pipe_fd: *mut [FD; 2]) -> KResult<()> {
    sys_pipe2(thread, pipe_fd, OpenFlags::empty())
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_GETDENTS)]
fn getdents(fd: FD, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    thread.files.get(fd).ok_or(EBADF)?.getdents(&mut buffer)?;
    Ok(buffer.wrote())
}

#[eonix_macros::define_syscall(SYS_GETDENTS64)]
fn getdents64(fd: FD, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    thread.files.get(fd).ok_or(EBADF)?.getdents64(&mut buffer)?;
    Ok(buffer.wrote())
}

#[cfg_attr(
    not(target_arch = "x86_64"),
    eonix_macros::define_syscall(SYS_NEWFSTATAT)
)]
#[cfg_attr(target_arch = "x86_64", eonix_macros::define_syscall(SYS_FSTATAT64))]
fn newfstatat(dirfd: FD, pathname: *const u8, statbuf: *mut Stat, flags: AtFlags) -> KResult<()> {
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
fn newfstat(fd: FD, statbuf: *mut Stat) -> KResult<()> {
    sys_newfstatat(
        thread,
        fd,
        core::ptr::null(),
        statbuf,
        AtFlags::AT_EMPTY_PATH,
    )
}

#[eonix_macros::define_syscall(SYS_STATX)]
fn statx(
    dirfd: FD,
    pathname: *const u8,
    flags: AtFlags,
    mask: u32,
    buffer: *mut StatX,
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
fn mkdirat(dirfd: FD, pathname: *const u8, mode: u32) -> KResult<()> {
    let umask = *thread.fs_context.umask.lock();
    let mode = mode & !umask & 0o777;

    let dentry = dentry_from(thread, dirfd, pathname, true)?;
    dentry.mkdir(mode)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_MKDIR)]
fn mkdir(pathname: *const u8, mode: u32) -> KResult<()> {
    sys_mkdirat(thread, FD::AT_FDCWD, pathname, mode)
}

#[eonix_macros::define_syscall(SYS_FTRUNCATE64)]
fn truncate64(fd: FD, length: usize) -> KResult<()> {
    let file = thread.files.get(fd).ok_or(EBADF)?;
    file.as_path().ok_or(EBADF)?.truncate(length)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_TRUNCATE)]
fn truncate(pathname: *const u8, length: usize) -> KResult<()> {
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&thread.fs_context, path, true)?;

    dentry.truncate(length)
}

#[eonix_macros::define_syscall(SYS_UNLINKAT)]
fn unlinkat(dirfd: FD, pathname: *const u8) -> KResult<()> {
    dentry_from(thread, dirfd, pathname, false)?.unlink()
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_UNLINK)]
fn unlink(pathname: *const u8) -> KResult<()> {
    sys_unlinkat(thread, FD::AT_FDCWD, pathname)
}

#[eonix_macros::define_syscall(SYS_SYMLINKAT)]
fn symlinkat(target: *const u8, dirfd: FD, linkpath: *const u8) -> KResult<()> {
    let target = UserString::new(target)?;
    let dentry = dentry_from(thread, dirfd, linkpath, false)?;

    dentry.symlink(target.as_cstr().to_bytes())
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_SYMLINK)]
fn symlink(target: *const u8, linkpath: *const u8) -> KResult<()> {
    sys_symlinkat(thread, target, FD::AT_FDCWD, linkpath)
}

#[eonix_macros::define_syscall(SYS_MKNODAT)]
fn mknodat(dirfd: FD, pathname: *const u8, mode: u32, dev: u32) -> KResult<()> {
    let dentry = dentry_from(thread, dirfd, pathname, true)?;

    let umask = *thread.fs_context.umask.lock();
    let mode = mode & ((!umask & 0o777) | (S_IFBLK | S_IFCHR));

    dentry.mknod(mode, dev)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_MKNOD)]
fn mknod(pathname: *const u8, mode: u32, dev: u32) -> KResult<()> {
    sys_mknodat(thread, FD::AT_FDCWD, pathname, mode, dev)
}

#[eonix_macros::define_syscall(SYS_READLINKAT)]
fn readlinkat(dirfd: FD, pathname: *const u8, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let dentry = dentry_from(thread, dirfd, pathname, false)?;
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    dentry.readlink(&mut buffer)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_READLINK)]
fn readlink(pathname: *const u8, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    sys_readlinkat(thread, FD::AT_FDCWD, pathname, buffer, bufsize)
}

fn do_lseek(thread: &Thread, fd: FD, offset: u64, whence: u32) -> KResult<u64> {
    let file = thread.files.get(fd).ok_or(EBADF)?;

    Ok(match whence {
        SEEK_SET => file.seek(SeekOption::Set(offset as usize))?,
        SEEK_CUR => file.seek(SeekOption::Current(offset as isize))?,
        SEEK_END => file.seek(SeekOption::End(offset as isize))?,
        _ => return Err(EINVAL),
    } as u64)
}

#[cfg(not(target_arch = "x86_64"))]
#[eonix_macros::define_syscall(SYS_LSEEK)]
fn lseek(fd: FD, offset: u64, whence: u32) -> KResult<u64> {
    do_lseek(thread, fd, offset, whence)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_LLSEEK)]
fn llseek(fd: FD, offset_high: u32, offset_low: u32, result: *mut u64, whence: u32) -> KResult<()> {
    let mut result = UserBuffer::new(result as *mut u8, core::mem::size_of::<u64>())?;
    let offset = ((offset_high as u64) << 32) | (offset_low as u64);

    let new_offset = do_lseek(thread, fd, offset, whence)?;

    result.copy(&new_offset)?.ok_or(EFAULT)
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct IoVec {
    pub base: PtrT,
    pub len: Long,
}

#[eonix_macros::define_syscall(SYS_READV)]
fn readv(fd: FD, iov_user: *const IoVec, iovcnt: u32) -> KResult<usize> {
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
            Ok(IoVec { base, len }) => Some(UserBuffer::new(base.addr() as *mut u8, len.get())),
        })
        .collect::<KResult<Vec<_>>>()?;

    let mut tot = 0usize;
    for mut buffer in iov_buffers.into_iter() {
        // TODO!!!: `readv`
        let nread = Task::block_on(file.read(&mut buffer, None))?;
        tot += nread;

        if nread != buffer.total() {
            break;
        }
    }

    Ok(tot)
}

#[eonix_macros::define_syscall(SYS_WRITEV)]
fn writev(fd: FD, iov_user: *const IoVec, iovcnt: u32) -> KResult<usize> {
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
                CheckedUserPointer::new(base.addr() as *mut u8, len.get())
                    .map(|ptr| ptr.into_stream()),
            ),
        })
        .collect::<KResult<Vec<_>>>()?;

    let mut tot = 0usize;
    for mut stream in iov_streams.into_iter() {
        let nread = Task::block_on(file.write(&mut stream, None))?;
        tot += nread;

        if nread == 0 || !stream.is_drained() {
            break;
        }
    }

    Ok(tot)
}

#[eonix_macros::define_syscall(SYS_FACCESSAT)]
fn faccessat(dirfd: FD, pathname: *const u8, _mode: u32, flags: AtFlags) -> KResult<()> {
    let dentry = if flags.at_empty_path() {
        let file = thread.files.get(dirfd).ok_or(EBADF)?;
        file.as_path().ok_or(EBADF)?.clone()
    } else {
        dentry_from(thread, dirfd, pathname, !flags.no_follow())?
    };

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
fn access(pathname: *const u8, mode: u32) -> KResult<()> {
    sys_faccessat(thread, FD::AT_FDCWD, pathname, mode, AtFlags::empty())
}

#[eonix_macros::define_syscall(SYS_SENDFILE64)]
fn sendfile64(out_fd: FD, in_fd: FD, offset: *mut u8, count: usize) -> KResult<usize> {
    let in_file = thread.files.get(in_fd).ok_or(EBADF)?;
    let out_file = thread.files.get(out_fd).ok_or(EBADF)?;

    if !offset.is_null() {
        unimplemented!("sendfile64 with offset");
    }

    Task::block_on(in_file.sendfile(&out_file, count))
}

#[eonix_macros::define_syscall(SYS_IOCTL)]
fn ioctl(fd: FD, request: usize, arg3: usize) -> KResult<usize> {
    let file = thread.files.get(fd).ok_or(EBADF)?;

    file.ioctl(request, arg3)
}

#[eonix_macros::define_syscall(SYS_FCNTL64)]
fn fcntl64(fd: FD, cmd: u32, arg: usize) -> KResult<usize> {
    thread.files.fcntl(fd, cmd, arg)
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct UserPollFd {
    fd: FD,
    events: u16,
    revents: u16,
}

fn do_poll(thread: &Thread, fds: *mut UserPollFd, nfds: u32, _timeout: u32) -> KResult<u32> {
    match nfds {
        0 => Ok(0),
        2.. => unimplemented!("Poll with {} fds", nfds),
        1 => {
            // TODO!!: Poll with timeout
            // if timeout != u32::MAX {
            //     unimplemented!("Poll with timeout {}", timeout);
            // }
            let fds = UserPointerMut::new(fds)?;
            let mut fd = fds.read()?;

            let file = thread.files.get(fd.fd).ok_or(EBADF)?;
            fd.revents = Task::block_on(file.poll(PollEvent::from_bits_retain(fd.events)))?.bits();

            fds.write(fd)?;
            Ok(1)
        }
    }
}

#[eonix_macros::define_syscall(SYS_PPOLL)]
fn ppoll(
    fds: *mut UserPollFd,
    nfds: u32,
    _timeout_ptr: *const TimeSpec,
    _sigmask: *const SigSet,
) -> KResult<u32> {
    // TODO: Implement ppoll with signal mask and timeout
    do_poll(thread, fds, nfds, 0)
}

#[eonix_macros::define_syscall(SYS_PSELECT6)]
fn pselect6(
    nfds: u32,
    readfds: *mut FDSet,
    writefds: *mut FDSet,
    _exceptfds: *mut FDSet,
    timeout: *mut TimeSpec,
    _sigmask: *const (),
) -> KResult<usize> {
    // According to [pthread6(2)](https://linux.die.net/man/2/pselect6):
    // Some code calls select() with all three sets empty, nfds zero, and
    // a non-NULL timeout as a fairly portable way to sleep with subsecond precision.
    if nfds == 0 {
        let timeout = UserPointerMut::new(timeout)?;

        // Read here to check for invalid pointers.
        let _timeout_value = timeout.read()?;

        Task::block_on(sleep(Duration::from_millis(10)));

        timeout.write(TimeSpec {
            tv_sec: 0,
            tv_nsec: 0,
        })?;

        return Ok(0);
    }

    let _time_out = if timeout.is_null() {
        None
    } else {
        let timeout = UserPointer::new(timeout)?.read()?;
        if timeout.tv_nsec == 0 && timeout.tv_sec == 0 {
            return Ok(0);
        }

        Some(timeout)
    };

    let mut read_fds = if readfds.is_null() {
        None
    } else {
        Some(UserPointer::new(readfds)?.read()?)
    };
    let mut write_fds = if writefds.is_null() {
        None
    } else {
        Some(UserPointer::new(writefds)?.read()?)
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

    loop {
        for (fd, events) in &poll_fds {
            let res = Task::block_on(thread.files.get(FD::from(*fd)).ok_or(EBADF)?.poll(*events))?;

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
        Task::block_on(sleep(Duration::from_millis(100)));
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
fn poll(fds: *mut UserPollFd, nfds: u32, timeout: u32) -> KResult<u32> {
    do_poll(thread, fds, nfds, timeout)
}

#[eonix_macros::define_syscall(SYS_FCHOWNAT)]
fn fchownat(dirfd: FD, pathname: *const u8, uid: u32, gid: u32, flags: AtFlags) -> KResult<()> {
    let dentry = dentry_from(thread, dirfd, pathname, !flags.no_follow())?;
    if !dentry.is_valid() {
        return Err(ENOENT);
    }

    dentry.chown(uid, gid)
}

#[eonix_macros::define_syscall(SYS_FCHMODAT)]
fn fchmodat(dirfd: FD, pathname: *const u8, mode: u32, flags: AtFlags) -> KResult<()> {
    let dentry = if flags.at_empty_path() {
        let file = thread.files.get(dirfd).ok_or(EBADF)?;
        file.as_path().ok_or(EBADF)?.clone()
    } else {
        dentry_from(thread, dirfd, pathname, !flags.no_follow())?
    };

    if !dentry.is_valid() {
        return Err(ENOENT);
    }

    dentry.chmod(mode)
}

#[eonix_macros::define_syscall(SYS_FCHMOD)]
fn chmod(pathname: *const u8, mode: u32) -> KResult<()> {
    sys_fchmodat(thread, FD::AT_FDCWD, pathname, mode, AtFlags::empty())
}

#[eonix_macros::define_syscall(SYS_UTIMENSAT)]
fn utimensat(
    dirfd: FD,
    pathname: *const u8,
    times: *const TimeSpec,
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
fn renameat2(
    old_dirfd: FD,
    old_pathname: *const u8,
    new_dirfd: FD,
    new_pathname: *const u8,
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

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_RENAME)]
fn rename(old_pathname: *const u8, new_pathname: *const u8) -> KResult<()> {
    sys_renameat2(
        thread,
        FD::AT_FDCWD,
        old_pathname,
        FD::AT_FDCWD,
        new_pathname,
        0,
    )
}

pub fn keep_alive() {}
