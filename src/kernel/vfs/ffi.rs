use crate::{
    io::{ByteBuffer, RawBuffer},
    kernel::block::BlockDevice,
    prelude::*,
};

use core::ffi::{c_char, c_void};

use alloc::sync::Arc;
use bindings::{dev_t, ino_t, mode_t, statx};

use crate::io::get_str_from_cstr;

use super::{
    bindings::{fs, EINVAL, EISDIR},
    dentry::Dentry,
    inode::Inode,
    s_isblk, s_ischr, s_isdir, s_isreg, DevId,
};

fn into_slice<'a>(buf: *const u8, bufsize: &usize) -> &'a [u8] {
    unsafe { core::slice::from_raw_parts(buf, *bufsize) }
}

fn into_mut_slice<'a>(buf: *mut u8, bufsize: &usize) -> &'a mut [u8] {
    unsafe { core::slice::from_raw_parts_mut(buf, *bufsize) }
}

macro_rules! map_err_ffi {
    ($error:expr) => {
        match $error {
            Ok(_) => 0,
            Err(e) => -(e as i32),
        }
    };
}

#[no_mangle]
pub extern "C" fn fs_mount(
    mountpoint: *const Dentry, // borrowed
    source: *const c_char,
    mountpoint_str: *const c_char,
    fstype: *const c_char,
    flags: u64,
    _data: *const c_void,
) -> i32 {
    let mountpoint = Dentry::from_raw(&mountpoint);

    let source = get_str_from_cstr(source).unwrap();
    let mountpoint_str = get_str_from_cstr(mountpoint_str).unwrap();
    let fstype = get_str_from_cstr(fstype).unwrap();

    // TODO: data
    match super::mount::do_mount(
        &mountpoint,
        source,
        mountpoint_str,
        fstype,
        flags,
        &[],
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i32),
    }
}

fn do_read(
    file: &Arc<Inode>,
    buffer: &mut [u8],
    offset: usize,
) -> KResult<usize> {
    let mode = { file.idata.lock().mode };

    match mode {
        mode if s_isdir(mode) => Err(EISDIR),
        mode if s_isreg(mode) => {
            let mut buffer = ByteBuffer::new(buffer);
            file.read(file, &mut buffer, offset)
        }
        mode if s_isblk(mode) => {
            let mut buffer = ByteBuffer::new(buffer);
            let device = BlockDevice::get(file.devid(file)?)?;

            Ok(device.read_some(offset, &mut buffer)?.allow_partial())
        }
        mode if s_ischr(mode) => {
            let devid = file.devid(file)?;

            let ret = unsafe {
                fs::char_device_read(
                    devid,
                    buffer.as_mut_ptr() as *mut _,
                    buffer.len(),
                    buffer.len(),
                )
            };

            if ret < 0 {
                Err(-ret as u32)
            } else {
                Ok(ret as usize)
            }
        }
        _ => Err(EINVAL),
    }
}

fn do_write(file: &Arc<Inode>, buffer: &[u8], offset: usize) -> KResult<usize> {
    let mode = file.idata.lock().mode;

    match mode {
        mode if s_isdir(mode) => Err(EISDIR),
        mode if s_isreg(mode) => file.write(file, buffer, offset),
        mode if s_isblk(mode) => Err(EINVAL), // TODO
        mode if s_ischr(mode) => {
            let devid = file.devid(file)?;

            let ret = unsafe {
                fs::char_device_write(
                    devid,
                    buffer.as_ptr() as *const _,
                    buffer.len(),
                )
            };

            if ret < 0 {
                Err(-ret as u32)
            } else {
                Ok(ret as usize)
            }
        }
        _ => Err(EINVAL),
    }
}

fn inode_from_raw<'lt>(file: &'lt mut *const Inode) -> BorrowedArc<'lt, Inode> {
    BorrowedArc::new(file)
}

#[no_mangle]
pub extern "C" fn fs_read(
    mut file: *const Inode, // borrowed
    buf: *mut u8,
    bufsize: usize,
    offset: usize,
    n: usize,
) -> isize {
    let file = inode_from_raw(&mut file);

    let bufsize = bufsize.min(n);
    let buffer = into_mut_slice(buf, &bufsize);

    match do_read(&file, buffer, offset) {
        Ok(n) => n as isize,
        Err(e) => -(e as isize),
    }
}

#[no_mangle]
pub extern "C" fn fs_write(
    mut file: *const Inode, // borrowed
    buf: *const u8,
    offset: usize,
    n: usize,
) -> isize {
    let file = inode_from_raw(&mut file);
    let buffer = into_slice(buf, &n);

    match do_write(&file, buffer, offset) {
        Ok(n) => n as isize,
        Err(e) => -(e as isize),
    }
}

#[no_mangle]
pub extern "C" fn fs_statx(
    mut file: *const Inode, // borrowed
    stat: *mut statx,
    mask: u32,
) -> i32 {
    map_err_ffi!((|| {
        let file = inode_from_raw(&mut file);
        let statx = unsafe { stat.as_mut() }.unwrap();

        file.statx(file.as_ref(), statx, mask)
    })())
}

#[no_mangle]
pub extern "C" fn fs_truncate(
    mut file: *const Inode, // borrowed
    size: usize,
) -> i32 {
    map_err_ffi!((|| {
        let file = inode_from_raw(&mut file);
        file.truncate(file.as_ref(), size)
    })())
}

#[no_mangle]
pub extern "C" fn fs_readlink(
    mut file: *const Inode, // borrowed
    mut buf: *mut u8,
    bufsize: usize,
) -> i32 {
    let file = inode_from_raw(&mut file);
    let mut buffer = RawBuffer::new_from_raw(&mut buf, bufsize);

    match file.readlink(file.as_ref(), &mut buffer) {
        Ok(n) => n as i32,
        Err(e) => -(e as i32),
    }
}

#[no_mangle]
pub extern "C" fn fs_creat(
    at: *const Dentry, // borrowed
    mode: mode_t,
) -> i32 {
    map_err_ffi!((|| {
        let at = Dentry::from_raw(&at);
        let parent = at.parent();
        let inode = parent.get_inode()?;

        inode.creat(inode.as_ref(), &at, mode as u32)
    })())
}

#[no_mangle]
pub extern "C" fn fs_mkdir(
    at: *const Dentry, // borrowed
    mode: mode_t,
) -> i32 {
    map_err_ffi!((|| {
        let at = Dentry::from_raw(&at);
        let parent = at.parent();
        let inode = parent.get_inode()?;

        inode.mkdir(inode.as_ref(), &at, mode as u32)
    })())
}

#[no_mangle]
pub extern "C" fn fs_mknod(
    at: *const Dentry, // borrowed
    mode: mode_t,
    dev: dev_t,
) -> i32 {
    map_err_ffi!((|| {
        let at = Dentry::from_raw(&at);
        let parent = at.parent();
        let inode = parent.get_inode()?;

        inode.mknod(inode.as_ref(), &at, mode as u32, dev as DevId)
    })())
}

#[no_mangle]
pub extern "C" fn fs_symlink(
    at: *const Dentry, // borrowed
    target: *const c_char,
) -> i32 {
    map_err_ffi!((|| {
        let at = Dentry::from_raw(&at);
        let parent = at.parent();
        let inode = parent.get_inode()?;

        inode.symlink(
            inode.as_ref(),
            &at,
            get_str_from_cstr(target)?.as_bytes(),
        )
    })())
}

#[no_mangle]
pub extern "C" fn fs_unlink(at: *const Dentry) -> i32 {
    map_err_ffi!((|| {
        let at = Dentry::from_raw(&at);
        let parent = at.parent();
        let inode = parent.get_inode()?;

        inode.unlink(inode.as_ref(), &at)
    })())
}

#[no_mangle]
pub extern "C" fn r_get_inode_mode(mut inode: *const Inode) -> mode_t {
    let inode = inode_from_raw(&mut inode);
    let idata = inode.idata.lock();

    idata.mode as _
}

#[no_mangle]
pub extern "C" fn r_get_inode_size(mut inode: *const Inode) -> mode_t {
    let inode = inode_from_raw(&mut inode);
    let idata = inode.idata.lock();

    idata.size as _
}

extern "C" {
    fn call_callback(
        callback: *const c_void,
        filename: *const c_char,
        filename_len: usize,
        ino: ino_t,
    ) -> i32;
}

#[no_mangle]
pub extern "C" fn fs_readdir(
    mut file: *const Inode, // borrowed
    offset: usize,
    callback: *const c_void,
) -> i64 {
    let inode = inode_from_raw(&mut file);

    let ret = inode.readdir(inode.as_ref(), offset, &|filename, ino| {
        let ret = unsafe {
            call_callback(
                callback,
                filename.as_ptr() as *const c_char,
                filename.len(),
                ino,
            )
        };

        match ret {
            0 => Ok(()),
            _ => Err(ret as u32),
        }
    });

    match ret {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
