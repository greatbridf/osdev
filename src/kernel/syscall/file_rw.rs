use crate::kernel::constants::{
    AT_FDCWD, AT_STATX_SYNC_AS_STAT, AT_STATX_SYNC_TYPE, AT_SYMLINK_NOFOLLOW, EBADF, ENOTDIR,
    SEEK_CUR, SEEK_END, SEEK_SET, S_IFBLK, S_IFCHR,
};
use crate::kernel::task::Thread;
use crate::{
    io::{Buffer, BufferFill},
    kernel::{
        constants::{AT_EMPTY_PATH, EFAULT, EINVAL, ENOENT},
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
use alloc::sync::Arc;
use core::mem::MaybeUninit;
use eonix_runtime::task::Task;
use posix_types::stat::{Stat, StatX};
use posix_types::syscall_no::*;

fn dentry_from(
    thread: &Thread,
    dirfd: u32,
    pathname: *const u8,
    follow_symlink: bool,
) -> KResult<Arc<Dentry>> {
    const _AT_FDCWD: u32 = AT_FDCWD as u32;
    let path = UserString::new(pathname)?;

    match (path.as_cstr().to_bytes_with_nul()[0], dirfd) {
        (b'/', _) | (_, _AT_FDCWD) => {
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
fn read(fd: u32, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    Task::block_on(thread.files.get(fd).ok_or(EBADF)?.read(&mut buffer))
}

#[eonix_macros::define_syscall(SYS_WRITE)]
fn write(fd: u32, buffer: *const u8, count: usize) -> KResult<usize> {
    let data = unsafe { core::slice::from_raw_parts(buffer, count) };

    Task::block_on(thread.files.get(fd).ok_or(EBADF)?.write(data))
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_OPEN)]
fn open(path: *const u8, flags: u32, mode: u32) -> KResult<u32> {
    let path = UserString::new(path)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let mode = mode & !*thread.fs_context.umask.lock();

    thread.files.open(&thread.fs_context, path, flags, mode)
}

#[eonix_macros::define_syscall(SYS_CLOSE)]
fn close(fd: u32) -> KResult<()> {
    thread.files.close(fd)
}

#[eonix_macros::define_syscall(SYS_DUP)]
fn dup(fd: u32) -> KResult<u32> {
    thread.files.dup(fd)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_DUP2)]
fn dup2(old_fd: u32, new_fd: u32) -> KResult<u32> {
    thread.files.dup_to(old_fd, new_fd, 0)
}

#[eonix_macros::define_syscall(SYS_PIPE2)]
fn pipe2(pipe_fd: *mut [u32; 2], flags: u32) -> KResult<()> {
    let mut buffer = UserBuffer::new(pipe_fd as *mut u8, core::mem::size_of::<[u32; 2]>())?;
    let (read_fd, write_fd) = thread.files.pipe(flags)?;

    buffer.copy(&[read_fd, write_fd])?.ok_or(EFAULT)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_PIPE)]
fn pipe(pipe_fd: *mut [u32; 2]) -> KResult<()> {
    sys_pipe2(thread, pipe_fd, 0)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_GETDENTS)]
fn getdents(fd: u32, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    thread.files.get(fd).ok_or(EBADF)?.getdents(&mut buffer)?;
    Ok(buffer.wrote())
}

#[eonix_macros::define_syscall(SYS_GETDENTS64)]
fn getdents64(fd: u32, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    thread.files.get(fd).ok_or(EBADF)?.getdents64(&mut buffer)?;
    Ok(buffer.wrote())
}

#[eonix_macros::define_syscall(SYS_NEWFSTATAT)]
fn newfstatat(dirfd: u32, pathname: *const u8, statbuf: *mut Stat, flags: u32) -> KResult<()> {
    let dentry = if (flags & AT_EMPTY_PATH) != 0 {
        let file = thread.files.get(dirfd).ok_or(EBADF)?;
        file.as_path().ok_or(EBADF)?.clone()
    } else {
        let follow_symlink = (flags & AT_SYMLINK_NOFOLLOW) != AT_SYMLINK_NOFOLLOW;
        dentry_from(thread, dirfd, pathname, follow_symlink)?
    };

    let statbuf = UserPointerMut::new(statbuf)?;

    let mut statx = StatX::default();
    dentry.statx(&mut statx, u32::MAX)?;

    statbuf.write(statx.into())?;

    Ok(())
}

#[eonix_macros::define_syscall(SYS_STATX)]
fn statx(dirfd: u32, path: *const u8, flags: u32, mask: u32, buffer: *mut u8) -> KResult<()> {
    if (flags & AT_STATX_SYNC_TYPE) != AT_STATX_SYNC_AS_STAT {
        unimplemented!("AT_STATX_SYNC_TYPE={:x}", flags & AT_STATX_SYNC_TYPE);
    }

    let mut stat: StatX = unsafe { MaybeUninit::zeroed().assume_init() };
    let mut buffer = UserBuffer::new(buffer, core::mem::size_of::<StatX>())?;

    if (flags & AT_EMPTY_PATH) != 0 {
        let file = thread.files.get(dirfd).ok_or(EBADF)?;
        file.statx(&mut stat, mask)?;
    } else {
        let path = UserString::new(path)?;
        let path = Path::new(path.as_cstr().to_bytes())?;

        let file;
        if dirfd != AT_FDCWD as u32 && !path.is_absolute() {
            let at = thread.files.get(dirfd).ok_or(EBADF)?;
            file = Dentry::open_at(
                &thread.fs_context,
                at.as_path().ok_or(EBADF)?,
                path,
                (flags & AT_SYMLINK_NOFOLLOW) != AT_SYMLINK_NOFOLLOW,
            )?;
        } else {
            file = Dentry::open(
                &thread.fs_context,
                path,
                (flags & AT_SYMLINK_NOFOLLOW) != AT_SYMLINK_NOFOLLOW,
            )?;
        }

        file.statx(&mut stat, mask)?;
    }

    buffer.copy(&stat)?.ok_or(EFAULT)
}

#[eonix_macros::define_syscall(SYS_MKDIRAT)]
fn mkdirat(dirfd: u32, pathname: *const u8, mode: u32) -> KResult<()> {
    let umask = *thread.fs_context.umask.lock();
    let mode = mode & !umask & 0o777;

    let dentry = dentry_from(thread, dirfd, pathname, true)?;
    dentry.mkdir(mode)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_MKDIR)]
fn mkdir(pathname: *const u8, mode: u32) -> KResult<()> {
    sys_mkdirat(thread, AT_FDCWD as u32, pathname, mode)
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
fn unlinkat(dirfd: u32, pathname: *const u8) -> KResult<()> {
    dentry_from(thread, dirfd, pathname, false)?.unlink()
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_UNLINK)]
fn unlink(pathname: *const u8) -> KResult<()> {
    sys_unlinkat(thread, AT_FDCWD as u32, pathname)
}

#[eonix_macros::define_syscall(SYS_SYMLINKAT)]
fn symlinkat(dirfd: u32, target: *const u8, linkpath: *const u8) -> KResult<()> {
    let target = UserString::new(target)?;
    let dentry = dentry_from(thread, dirfd, linkpath, false)?;

    dentry.symlink(target.as_cstr().to_bytes())
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_SYMLINK)]
fn symlink(target: *const u8, linkpath: *const u8) -> KResult<()> {
    sys_symlinkat(thread, AT_FDCWD as u32, target, linkpath)
}

#[eonix_macros::define_syscall(SYS_MKNODAT)]
fn mknodat(dirfd: u32, pathname: *const u8, mode: u32, dev: u32) -> KResult<()> {
    let dentry = dentry_from(thread, dirfd, pathname, true)?;

    let umask = *thread.fs_context.umask.lock();
    let mode = mode & ((!umask & 0o777) | (S_IFBLK | S_IFCHR));

    dentry.mknod(mode, dev)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_MKNOD)]
fn mknod(pathname: *const u8, mode: u32, dev: u32) -> KResult<()> {
    sys_mknodat(thread, AT_FDCWD as u32, pathname, mode, dev)
}

#[eonix_macros::define_syscall(SYS_READLINKAT)]
fn readlinkat(dirfd: u32, pathname: *const u8, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let dentry = dentry_from(thread, dirfd, pathname, false)?;
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    dentry.readlink(&mut buffer)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_READLINK)]
fn readlink(pathname: *const u8, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    sys_readlinkat(thread, AT_FDCWD as u32, pathname, buffer, bufsize)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_LLSEEK)]
fn llseek(
    fd: u32,
    offset_high: u32,
    offset_low: u32,
    result: *mut u64,
    whence: u32,
) -> KResult<()> {
    let mut result = UserBuffer::new(result as *mut u8, core::mem::size_of::<u64>())?;
    let file = thread.files.get(fd).ok_or(EBADF)?;

    let offset = ((offset_high as u64) << 32) | offset_low as u64;

    let new_offset = match whence {
        SEEK_SET => file.seek(SeekOption::Set(offset as usize))?,
        SEEK_CUR => file.seek(SeekOption::Current(offset as isize))?,
        SEEK_END => file.seek(SeekOption::End(offset as isize))?,
        _ => return Err(EINVAL),
    } as u64;

    result.copy(&new_offset)?.ok_or(EFAULT)
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct IoVec32 {
    base: u32,
    len: u32,
}

#[eonix_macros::define_syscall(SYS_READV)]
fn readv(fd: u32, iov_user: *const IoVec32, iovcnt: u32) -> KResult<usize> {
    let file = thread.files.get(fd).ok_or(EBADF)?;

    let mut iov_user = UserPointer::new(iov_user as *mut IoVec32)?;
    let iov_buffers = (0..iovcnt)
        .map(|_| {
            let iov_result = iov_user.read()?;
            iov_user = iov_user.offset(1)?;
            Ok(iov_result)
        })
        .filter_map(|iov_result| match iov_result {
            Err(err) => Some(Err(err)),
            Ok(IoVec32 { len: 0, .. }) => None,
            Ok(IoVec32 { base, len }) => Some(UserBuffer::new(base as *mut u8, len as usize)),
        })
        .collect::<KResult<Vec<_>>>()?;

    let mut tot = 0usize;
    for mut buffer in iov_buffers.into_iter() {
        // TODO!!!: `readv`
        let nread = Task::block_on(file.read(&mut buffer))?;
        tot += nread;

        if nread != buffer.total() {
            break;
        }
    }

    Ok(tot)
}

#[eonix_macros::define_syscall(SYS_WRITEV)]
fn writev(fd: u32, iov_user: *const u8, iovcnt: u32) -> KResult<usize> {
    let file = thread.files.get(fd).ok_or(EBADF)?;

    // TODO: Rewrite this with `UserPointer`.
    let iov_user =
        CheckedUserPointer::new(iov_user, iovcnt as usize * core::mem::size_of::<IoVec32>())?;
    let mut iov_user_copied: Vec<IoVec32> = vec![];
    iov_user_copied.resize(iovcnt as usize, IoVec32::default());

    iov_user.read(
        iov_user_copied.as_mut_ptr() as *mut (),
        iov_user_copied.len() * core::mem::size_of::<IoVec32>(),
    )?;

    let iov_blocks = iov_user_copied
        .into_iter()
        .filter(|iov| iov.len != 0)
        .map(|iov| CheckedUserPointer::new(iov.base as *mut u8, iov.len as usize))
        .collect::<KResult<Vec<_>>>()?;

    let mut tot = 0usize;
    for block in iov_blocks.into_iter() {
        // TODO!!!: atomic `writev`
        // TODO!!!!!: copy from user
        let slice = block.as_slice();
        let nread = Task::block_on(file.write(slice))?;
        tot += nread;

        if nread == 0 || nread != slice.len() {
            break;
        }
    }

    Ok(tot)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_ACCESS)]
fn access(pathname: *const u8, _mode: u32) -> KResult<()> {
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&thread.fs_context, path, true)?;

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

#[eonix_macros::define_syscall(SYS_SENDFILE64)]
fn sendfile64(out_fd: u32, in_fd: u32, offset: *mut u8, count: usize) -> KResult<usize> {
    let in_file = thread.files.get(in_fd).ok_or(EBADF)?;
    let out_file = thread.files.get(out_fd).ok_or(EBADF)?;

    if !offset.is_null() {
        unimplemented!("sendfile64 with offset");
    }

    Task::block_on(in_file.sendfile(&out_file, count))
}

#[eonix_macros::define_syscall(SYS_IOCTL)]
fn ioctl(fd: u32, request: usize, arg3: usize) -> KResult<usize> {
    let file = thread.files.get(fd).ok_or(EBADF)?;

    file.ioctl(request, arg3)
}

#[eonix_macros::define_syscall(SYS_FCNTL64)]
fn fcntl64(fd: u32, cmd: u32, arg: usize) -> KResult<usize> {
    thread.files.fcntl(fd, cmd, arg)
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct UserPollFd {
    fd: u32,
    events: u16,
    revents: u16,
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_POLL)]
fn poll(fds: *mut UserPollFd, nfds: u32, _timeout: u32) -> KResult<u32> {
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

pub fn keep_alive() {}
