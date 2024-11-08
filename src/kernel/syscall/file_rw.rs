use core::mem::MaybeUninit;

use bindings::{
    statx, AT_FDCWD, AT_STATX_SYNC_AS_STAT, AT_STATX_SYNC_TYPE, AT_SYMLINK_NOFOLLOW, EBADF, EFAULT,
    EINVAL, ENOENT, SEEK_CUR, SEEK_END, SEEK_SET, S_IFBLK, S_IFCHR,
};

use crate::{
    io::{Buffer, BufferFill},
    kernel::{
        user::dataflow::{CheckedUserPointer, UserBuffer, UserString},
        vfs::{dentry::Dentry, file::SeekOption, filearray::FileArray, FsContext},
    },
    path::Path,
    prelude::*,
};

use super::{define_syscall32, register_syscall_handler};

fn do_read(fd: u32, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;
    let files = FileArray::get_current();

    files.get(fd).ok_or(EBADF)?.read(&mut buffer)
}

fn do_write(fd: u32, buffer: *const u8, count: usize) -> KResult<usize> {
    let data = unsafe { core::slice::from_raw_parts(buffer, count) };
    let files = FileArray::get_current();

    files.get(fd).ok_or(EBADF)?.write(data)
}

fn do_open(path: *const u8, flags: u32, mode: u32) -> KResult<u32> {
    let path = UserString::new(path)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let files = FileArray::get_current();
    let context = FsContext::get_current();
    let mode = mode & !*context.umask.lock();

    files.open(&context, path, flags, mode)
}

fn do_close(fd: u32) -> KResult<()> {
    let files = FileArray::get_current();
    files.close(fd)
}

fn do_dup(fd: u32) -> KResult<u32> {
    let files = FileArray::get_current();
    files.dup(fd)
}

fn do_dup2(old_fd: u32, new_fd: u32) -> KResult<u32> {
    let files = FileArray::get_current();
    files.dup_to(old_fd, new_fd, 0)
}

fn do_pipe(pipe_fd: *mut [u32; 2]) -> KResult<()> {
    let mut buffer = UserBuffer::new(pipe_fd as *mut u8, core::mem::size_of::<[u32; 2]>())?;
    let files = FileArray::get_current();
    let (read_fd, write_fd) = files.pipe()?;

    buffer.copy(&[read_fd, write_fd])?.ok_or(EFAULT)
}

fn do_getdents(fd: u32, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;
    let files = FileArray::get_current();

    files.get(fd).ok_or(EBADF)?.getdents(&mut buffer)?;
    Ok(buffer.wrote())
}

fn do_getdents64(fd: u32, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;
    let files = FileArray::get_current();

    files.get(fd).ok_or(EBADF)?.getdents64(&mut buffer)?;
    Ok(buffer.wrote())
}

fn do_statx(dirfd: u32, path: *const u8, flags: u32, mask: u32, buffer: *mut u8) -> KResult<()> {
    if (flags & AT_STATX_SYNC_TYPE) != AT_STATX_SYNC_AS_STAT {
        unimplemented!("AT_STATX_SYNC_TYPE={:x}", flags & AT_STATX_SYNC_TYPE);
    }

    if dirfd != AT_FDCWD as u32 {
        unimplemented!("dirfd={}", dirfd);
    }

    let path = UserString::new(path)?;
    let path = Path::new(path.as_cstr().to_bytes())?;
    let mut buffer = UserBuffer::new(buffer, core::mem::size_of::<statx>())?;

    let file = Dentry::open(
        &FsContext::get_current(),
        path,
        (flags & AT_SYMLINK_NOFOLLOW) != AT_SYMLINK_NOFOLLOW,
    )?;

    let mut stat: statx = unsafe { MaybeUninit::zeroed().assume_init() };

    file.statx(&mut stat, mask)?;
    buffer.copy(&stat)?.ok_or(EFAULT)
}

fn do_mkdir(pathname: *const u8, mode: u32) -> KResult<()> {
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let context = FsContext::get_current();
    let mode = mode & !*context.umask.lock() & 0o777;

    let dentry = Dentry::open(&context, path, true)?;

    dentry.mkdir(mode)
}

fn do_truncate(pathname: *const u8, length: usize) -> KResult<()> {
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&FsContext::get_current(), path, true)?;

    dentry.truncate(length)
}

fn do_unlink(pathname: *const u8) -> KResult<()> {
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&FsContext::get_current(), path, false)?;

    dentry.unlink()
}

fn do_symlink(target: *const u8, linkpath: *const u8) -> KResult<()> {
    let target = UserString::new(target)?;
    let linkpath = UserString::new(linkpath)?;
    let linkpath = Path::new(linkpath.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&FsContext::get_current(), linkpath, false)?;

    dentry.symlink(target.as_cstr().to_bytes())
}

fn do_mknod(pathname: *const u8, mode: u32, dev: u32) -> KResult<()> {
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let context = FsContext::get_current();
    let mode = mode & ((!*context.umask.lock() & 0o777) | (S_IFBLK | S_IFCHR));

    let dentry = Dentry::open(&context, path, true)?;

    dentry.mknod(mode, dev)
}

fn do_readlink(pathname: *const u8, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&FsContext::get_current(), path, false)?;

    let mut buffer = UserBuffer::new(buffer, bufsize)?;
    dentry.readlink(&mut buffer)
}

fn do_llseek(
    fd: u32,
    offset_high: u32,
    offset_low: u32,
    result: *mut u64,
    whence: u32,
) -> KResult<()> {
    let mut result = UserBuffer::new(result as *mut u8, core::mem::size_of::<u64>())?;
    let files = FileArray::get_current();
    let file = files.get(fd).ok_or(EBADF)?;

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

fn do_readv(fd: u32, iov_user: *const u8, iovcnt: u32) -> KResult<usize> {
    let files = FileArray::get_current();
    let file = files.get(fd).ok_or(EBADF)?;

    let iov_user =
        CheckedUserPointer::new(iov_user, iovcnt as usize * core::mem::size_of::<IoVec32>())?;
    let mut iov_user_copied: Vec<IoVec32> = vec![];
    iov_user_copied.resize(iovcnt as usize, IoVec32::default());

    iov_user.read(
        iov_user_copied.as_mut_ptr() as *mut (),
        iov_user_copied.len() * core::mem::size_of::<IoVec32>(),
    )?;

    let iov_buffers = iov_user_copied
        .into_iter()
        .take_while(|iov| iov.len != 0)
        .map(|iov| UserBuffer::new(iov.base as *mut u8, iov.len as usize))
        .collect::<KResult<Vec<_>>>()?;

    let mut tot = 0usize;
    for mut buffer in iov_buffers.into_iter() {
        // TODO!!!: `readv`
        let nread = file.read(&mut buffer)?;
        tot += nread;

        if nread == 0 || nread != buffer.total() {
            break;
        }
    }

    Ok(tot)
}

fn do_writev(fd: u32, iov_user: *const u8, iovcnt: u32) -> KResult<usize> {
    let files = FileArray::get_current();
    let file = files.get(fd).ok_or(EBADF)?;

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
        let nread = file.write(slice)?;
        tot += nread;

        if nread == 0 || nread != slice.len() {
            break;
        }
    }

    Ok(tot)
}

fn do_access(pathname: *const u8, _mode: u32) -> KResult<()> {
    let path = UserString::new(pathname)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&FsContext::get_current(), path, true)?;

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

fn do_sendfile64(out_fd: u32, in_fd: u32, offset: *mut u8, count: usize) -> KResult<usize> {
    let files = FileArray::get_current();
    let in_file = files.get(in_fd).ok_or(EBADF)?;
    let out_file = files.get(out_fd).ok_or(EBADF)?;

    if !offset.is_null() {
        unimplemented!("sendfile64 with offset");
    }

    in_file.sendfile(&out_file, count)
}

fn do_ioctl(fd: u32, request: usize, arg3: usize) -> KResult<usize> {
    let files = FileArray::get_current();
    let file = files.get(fd).ok_or(EBADF)?;

    file.ioctl(request, arg3)
}

fn do_fcntl64(fd: u32, cmd: u32, arg: usize) -> KResult<usize> {
    FileArray::get_current().fcntl(fd, cmd, arg)
}

define_syscall32!(sys_read, do_read, fd: u32, buffer: *mut u8, bufsize: usize);
define_syscall32!(sys_write, do_write, fd: u32, buffer: *const u8, count: usize);
define_syscall32!(sys_open, do_open, path: *const u8, flags: u32, mode: u32);
define_syscall32!(sys_close, do_close, fd: u32);
define_syscall32!(sys_dup, do_dup, fd: u32);
define_syscall32!(sys_dup2, do_dup2, old_fd: u32, new_fd: u32);
define_syscall32!(sys_pipe, do_pipe, pipe_fd: *mut [u32; 2]);
define_syscall32!(sys_getdents, do_getdents, fd: u32, buffer: *mut u8, bufsize: usize);
define_syscall32!(sys_getdents64, do_getdents64, fd: u32, buffer: *mut u8, bufsize: usize);
define_syscall32!(sys_statx, do_statx, fd: u32, path: *const u8, flags: u32, mask: u32, buffer: *mut u8);
define_syscall32!(sys_mkdir, do_mkdir, pathname: *const u8, mode: u32);
define_syscall32!(sys_truncate, do_truncate, pathname: *const u8, length: usize);
define_syscall32!(sys_unlink, do_unlink, pathname: *const u8);
define_syscall32!(sys_symlink, do_symlink, target: *const u8, linkpath: *const u8);
define_syscall32!(sys_readlink, do_readlink, pathname: *const u8, buffer: *mut u8, bufsize: usize);
define_syscall32!(sys_llseek, do_llseek, fd: u32, offset_high: u32, offset_low: u32, result: *mut u64, whence: u32);
define_syscall32!(sys_mknod, do_mknod, pathname: *const u8, mode: u32, dev: u32);
define_syscall32!(sys_readv, do_readv, fd: u32, iov_user: *const u8, iovcnt: u32);
define_syscall32!(sys_writev, do_writev, fd: u32, iov_user: *const u8, iovcnt: u32);
define_syscall32!(sys_access, do_access, pathname: *const u8, mode: u32);
define_syscall32!(sys_sendfile64, do_sendfile64, out_fd: u32, in_fd: u32, offset: *mut u8, count: usize);
define_syscall32!(sys_ioctl, do_ioctl, fd: u32, request: usize, arg3: usize);
define_syscall32!(sys_fcntl64, do_fcntl64, fd: u32, cmd: u32, arg: usize);

pub(super) unsafe fn register() {
    register_syscall_handler(0x03, sys_read, b"read\0".as_ptr() as *const _);
    register_syscall_handler(0x04, sys_write, b"write\0".as_ptr() as *const _);
    register_syscall_handler(0x05, sys_open, b"open\0".as_ptr() as *const _);
    register_syscall_handler(0x06, sys_close, b"close\0".as_ptr() as *const _);
    register_syscall_handler(0x0a, sys_unlink, b"unlink\0".as_ptr() as *const _);
    register_syscall_handler(0x0e, sys_mknod, b"mknod\0".as_ptr() as *const _);
    register_syscall_handler(0x21, sys_access, b"access\0".as_ptr() as *const _);
    register_syscall_handler(0x27, sys_mkdir, b"mkdir\0".as_ptr() as *const _);
    register_syscall_handler(0x29, sys_dup, b"dup\0".as_ptr() as *const _);
    register_syscall_handler(0x2a, sys_pipe, b"pipe\0".as_ptr() as *const _);
    register_syscall_handler(0x36, sys_ioctl, b"ioctl\0".as_ptr() as *const _);
    register_syscall_handler(0x3f, sys_dup2, b"dup2\0".as_ptr() as *const _);
    register_syscall_handler(0x53, sys_symlink, b"symlink\0".as_ptr() as *const _);
    register_syscall_handler(0x55, sys_readlink, b"readlink\0".as_ptr() as *const _);
    register_syscall_handler(0x5c, sys_truncate, b"truncate\0".as_ptr() as *const _);
    register_syscall_handler(0x8c, sys_llseek, b"llseek\0".as_ptr() as *const _);
    register_syscall_handler(0x8d, sys_getdents, b"getdents\0".as_ptr() as *const _);
    register_syscall_handler(0x91, sys_readv, b"readv\0".as_ptr() as *const _);
    register_syscall_handler(0x92, sys_writev, b"writev\0".as_ptr() as *const _);
    register_syscall_handler(0xdc, sys_getdents64, b"getdents64\0".as_ptr() as *const _);
    register_syscall_handler(0xdd, sys_fcntl64, b"fcntl64\0".as_ptr() as *const _);
    register_syscall_handler(0xef, sys_sendfile64, b"sendfile64\0".as_ptr() as *const _);
    register_syscall_handler(0x17f, sys_statx, b"statx\0".as_ptr() as *const _);
}
