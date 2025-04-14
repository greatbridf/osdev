use core::mem::MaybeUninit;

use bindings::{
    statx, AT_FDCWD, AT_STATX_SYNC_AS_STAT, AT_STATX_SYNC_TYPE, AT_SYMLINK_NOFOLLOW, EBADF, EFAULT,
    EINVAL, ENOENT, SEEK_CUR, SEEK_END, SEEK_SET, S_IFBLK, S_IFCHR,
};
use eonix_runtime::task::Task;

use crate::{
    io::{Buffer, BufferFill},
    kernel::{
        constants::AT_EMPTY_PATH,
        task::Thread,
        user::{
            dataflow::{CheckedUserPointer, UserBuffer, UserString},
            UserPointer, UserPointerMut,
        },
        vfs::{
            dentry::Dentry,
            file::{PollEvent, SeekOption},
            filearray::FileArray,
            FsContext,
        },
    },
    path::Path,
    prelude::*,
};

use super::{define_syscall32, register_syscall};

fn do_read(fd: u32, buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let mut buffer = UserBuffer::new(buffer, bufsize)?;
    let files = FileArray::get_current();

    Task::block_on(files.get(fd).ok_or(EBADF)?.read(&mut buffer))
}

fn do_write(fd: u32, buffer: *const u8, count: usize) -> KResult<usize> {
    let data = unsafe { core::slice::from_raw_parts(buffer, count) };
    let files = FileArray::get_current();

    Task::block_on(files.get(fd).ok_or(EBADF)?.write(data))
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

fn do_pipe2(pipe_fd: *mut [u32; 2], flags: u32) -> KResult<()> {
    let mut buffer = UserBuffer::new(pipe_fd as *mut u8, core::mem::size_of::<[u32; 2]>())?;
    let files = FileArray::get_current();
    let (read_fd, write_fd) = files.pipe(flags)?;

    buffer.copy(&[read_fd, write_fd])?.ok_or(EFAULT)
}

fn do_pipe(pipe_fd: *mut [u32; 2]) -> KResult<()> {
    do_pipe2(pipe_fd, 0)
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

    let mut stat: statx = unsafe { MaybeUninit::zeroed().assume_init() };
    let mut buffer = UserBuffer::new(buffer, core::mem::size_of::<statx>())?;

    if (flags & AT_EMPTY_PATH) != 0 {
        let file = FileArray::get_current().get(dirfd).ok_or(EBADF)?;
        file.statx(&mut stat, mask)?;
    } else {
        let path = UserString::new(path)?;
        let path = Path::new(path.as_cstr().to_bytes())?;

        let file;
        if dirfd != AT_FDCWD as u32 && !path.is_absolute() {
            let at = FileArray::get_current().get(dirfd).ok_or(EBADF)?;
            file = Dentry::open_at(
                &FsContext::get_current(),
                at.as_path().ok_or(EBADF)?,
                path,
                (flags & AT_SYMLINK_NOFOLLOW) != AT_SYMLINK_NOFOLLOW,
            )?;
        } else {
            file = Dentry::open(
                &FsContext::get_current(),
                path,
                (flags & AT_SYMLINK_NOFOLLOW) != AT_SYMLINK_NOFOLLOW,
            )?;
        }

        file.statx(&mut stat, mask)?;
    }

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

fn do_readv(fd: u32, iov_user: *const IoVec32, iovcnt: u32) -> KResult<usize> {
    let files = FileArray::get_current();
    let file = files.get(fd).ok_or(EBADF)?;

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

fn do_writev(fd: u32, iov_user: *const u8, iovcnt: u32) -> KResult<usize> {
    let files = FileArray::get_current();
    let file = files.get(fd).ok_or(EBADF)?;

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

    Task::block_on(in_file.sendfile(&out_file, count))
}

fn do_ioctl(fd: u32, request: usize, arg3: usize) -> KResult<usize> {
    let files = FileArray::get_current();
    let file = files.get(fd).ok_or(EBADF)?;

    file.ioctl(request, arg3)
}

fn do_fcntl64(fd: u32, cmd: u32, arg: usize) -> KResult<usize> {
    FileArray::get_current().fcntl(fd, cmd, arg)
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct UserPollFd {
    fd: u32,
    events: u16,
    revents: u16,
}

fn do_poll(fds: *mut UserPollFd, nfds: u32, _timeout: u32) -> KResult<u32> {
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

            let file = Thread::current().files.get(fd.fd).ok_or(EBADF)?;
            fd.revents = Task::block_on(file.poll(PollEvent::from_bits_retain(fd.events)))?.bits();

            fds.write(fd)?;
            Ok(1)
        }
    }
}

define_syscall32!(sys_read, do_read, fd: u32, buffer: *mut u8, bufsize: usize);
define_syscall32!(sys_write, do_write, fd: u32, buffer: *const u8, count: usize);
define_syscall32!(sys_open, do_open, path: *const u8, flags: u32, mode: u32);
define_syscall32!(sys_close, do_close, fd: u32);
define_syscall32!(sys_dup, do_dup, fd: u32);
define_syscall32!(sys_dup2, do_dup2, old_fd: u32, new_fd: u32);
define_syscall32!(sys_pipe, do_pipe, pipe_fd: *mut [u32; 2]);
define_syscall32!(sys_pipe2, do_pipe2, pipe_fd: *mut [u32; 2], flags: u32);
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
define_syscall32!(sys_readv, do_readv, fd: u32, iov_user: *const IoVec32, iovcnt: u32);
define_syscall32!(sys_writev, do_writev, fd: u32, iov_user: *const u8, iovcnt: u32);
define_syscall32!(sys_access, do_access, pathname: *const u8, mode: u32);
define_syscall32!(sys_sendfile64, do_sendfile64, out_fd: u32, in_fd: u32, offset: *mut u8, count: usize);
define_syscall32!(sys_ioctl, do_ioctl, fd: u32, request: usize, arg3: usize);
define_syscall32!(sys_fcntl64, do_fcntl64, fd: u32, cmd: u32, arg: usize);
define_syscall32!(sys_poll, do_poll, fds: *mut UserPollFd, nfds: u32, timeout: u32);

pub(super) fn register() {
    register_syscall!(0x03, read);
    register_syscall!(0x04, write);
    register_syscall!(0x05, open);
    register_syscall!(0x06, close);
    register_syscall!(0x0a, unlink);
    register_syscall!(0x0e, mknod);
    register_syscall!(0x21, access);
    register_syscall!(0x27, mkdir);
    register_syscall!(0x29, dup);
    register_syscall!(0x2a, pipe);
    register_syscall!(0x36, ioctl);
    register_syscall!(0x3f, dup2);
    register_syscall!(0x53, symlink);
    register_syscall!(0x55, readlink);
    register_syscall!(0x5c, truncate);
    register_syscall!(0x8c, llseek);
    register_syscall!(0x8d, getdents);
    register_syscall!(0x91, readv);
    register_syscall!(0x92, writev);
    register_syscall!(0xa8, poll);
    register_syscall!(0xdc, getdents64);
    register_syscall!(0xdd, fcntl64);
    register_syscall!(0xef, sendfile64);
    register_syscall!(0x14b, pipe2);
    register_syscall!(0x17f, statx);
}
