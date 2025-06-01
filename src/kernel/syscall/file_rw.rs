use crate::kernel::constants::{
    AT_FDCWD, AT_STATX_SYNC_AS_STAT, AT_STATX_SYNC_TYPE, AT_SYMLINK_NOFOLLOW, EBADF, SEEK_CUR,
    SEEK_END, SEEK_SET, S_IFBLK, S_IFCHR,
};
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
use core::mem::MaybeUninit;
use eonix_runtime::task::Task;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct StatXTimestamp {
    pub tv_sec: i64,
    pub tv_nsec: u32,
    pub __reserved: i32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct StatX {
    pub stx_mask: u32,
    pub stx_blksize: u32,
    pub stx_attributes: u64,
    pub stx_nlink: u32,
    pub stx_uid: u32,
    pub stx_gid: u32,
    pub stx_mode: u16,
    pub __spare0: [u16; 1usize],
    pub stx_ino: u64,
    pub stx_size: u64,
    pub stx_blocks: u64,
    pub stx_attributes_mask: u64,
    pub stx_atime: StatXTimestamp,
    pub stx_btime: StatXTimestamp,
    pub stx_ctime: StatXTimestamp,
    pub stx_mtime: StatXTimestamp,
    pub stx_rdev_major: u32,
    pub stx_rdev_minor: u32,
    pub stx_dev_major: u32,
    pub stx_dev_minor: u32,
    pub stx_mnt_id: u64,
    pub stx_dio_alignment: [u64; 13usize],
}

#[eonix_macros::define_syscall(0x03)]
fn read(fd: u32, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    Task::block_on(thread.files.get(fd).ok_or(EBADF)?.read(&mut buffer))
}

#[eonix_macros::define_syscall(0x04)]
fn write(fd: u32, buffer: *const u8, count: usize) -> KResult<usize> {
    let data = unsafe { core::slice::from_raw_parts(buffer, count) };

    Task::block_on(thread.files.get(fd).ok_or(EBADF)?.write(data))
}

#[eonix_macros::define_syscall(0x05)]
fn open(path: *const u8, flags: u32, mode: u32) -> KResult<u32> {
    let path = UserString::new(path)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let mode = mode & !*thread.fs_context.umask.lock();

    thread.files.open(&thread.fs_context, path, flags, mode)
}

#[eonix_macros::define_syscall(0x06)]
fn close(fd: u32) -> KResult<()> {
    thread.files.close(fd)
}

#[eonix_macros::define_syscall(0x29)]
fn dup(fd: u32) -> KResult<u32> {
    thread.files.dup(fd)
}

#[eonix_macros::define_syscall(0x3f)]
fn dup2(old_fd: u32, new_fd: u32) -> KResult<u32> {
    thread.files.dup_to(old_fd, new_fd, 0)
}

#[eonix_macros::define_syscall(0x14b)]
fn pipe2(pipe_fd: *mut [u32; 2], flags: u32) -> KResult<()> {
    let mut buffer = UserBuffer::new(pipe_fd as *mut u8, core::mem::size_of::<[u32; 2]>())?;
    let (read_fd, write_fd) = thread.files.pipe(flags)?;

    buffer.copy(&[read_fd, write_fd])?.ok_or(EFAULT)
}

#[eonix_macros::define_syscall(0x2a)]
fn pipe(pipe_fd: *mut [u32; 2]) -> KResult<()> {
    sys_pipe2(thread, pipe_fd, 0)
}

#[eonix_macros::define_syscall(0x8d)]
fn getdents(fd: u32, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    thread.files.get(fd).ok_or(EBADF)?.getdents(&mut buffer)?;
    Ok(buffer.wrote())
}

#[eonix_macros::define_syscall(0xdc)]
fn getdents64(fd: u32, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    thread.files.get(fd).ok_or(EBADF)?.getdents64(&mut buffer)?;
    Ok(buffer.wrote())
}

#[eonix_macros::define_syscall(0x17f)]
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

#[eonix_macros::define_syscall(0x27)]
fn mkdir(pathname: *const u8, mode: u32) -> KResult<()> {
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let umask = *thread.fs_context.umask.lock();
    let mode = mode & !umask & 0o777;

    let dentry = Dentry::open(&thread.fs_context, path, true)?;

    dentry.mkdir(mode)
}

#[eonix_macros::define_syscall(0x5c)]
fn truncate(pathname: *const u8, length: usize) -> KResult<()> {
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&thread.fs_context, path, true)?;

    dentry.truncate(length)
}

#[eonix_macros::define_syscall(0x0a)]
fn unlink(pathname: *const u8) -> KResult<()> {
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&thread.fs_context, path, false)?;

    dentry.unlink()
}

#[eonix_macros::define_syscall(0x53)]
fn symlink(target: *const u8, linkpath: *const u8) -> KResult<()> {
    let target = UserString::new(target)?;
    let linkpath = UserString::new(linkpath)?;
    let linkpath = Path::new(linkpath.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&thread.fs_context, linkpath, false)?;

    dentry.symlink(target.as_cstr().to_bytes())
}

#[eonix_macros::define_syscall(0x0e)]
fn mknod(pathname: *const u8, mode: u32, dev: u32) -> KResult<()> {
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let umask = *thread.fs_context.umask.lock();
    let mode = mode & ((!umask & 0o777) | (S_IFBLK | S_IFCHR));

    let dentry = Dentry::open(&thread.fs_context, path, true)?;

    dentry.mknod(mode, dev)
}

#[eonix_macros::define_syscall(0x55)]
fn readlink(pathname: *const u8, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&thread.fs_context, path, false)?;

    let mut buffer = UserBuffer::new(buffer, bufsize)?;
    dentry.readlink(&mut buffer)
}

#[eonix_macros::define_syscall(0x8c)]
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

#[eonix_macros::define_syscall(0x91)]
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

#[eonix_macros::define_syscall(0x92)]
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

#[eonix_macros::define_syscall(0x21)]
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

#[eonix_macros::define_syscall(0xef)]
fn sendfile64(out_fd: u32, in_fd: u32, offset: *mut u8, count: usize) -> KResult<usize> {
    let in_file = thread.files.get(in_fd).ok_or(EBADF)?;
    let out_file = thread.files.get(out_fd).ok_or(EBADF)?;

    if !offset.is_null() {
        unimplemented!("sendfile64 with offset");
    }

    Task::block_on(in_file.sendfile(&out_file, count))
}

#[eonix_macros::define_syscall(0x36)]
fn ioctl(fd: u32, request: usize, arg3: usize) -> KResult<usize> {
    let file = thread.files.get(fd).ok_or(EBADF)?;

    file.ioctl(request, arg3)
}

#[eonix_macros::define_syscall(0xdd)]
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

#[eonix_macros::define_syscall(0xa8)]
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
