use crate::{io::ByteBuffer, kernel::block::BlockDevice, prelude::*};

use core::ffi::{c_char, c_void};

use alloc::sync::Arc;
use bindings::{dev_t, fs::D_PRESENT, mode_t, statx};

use crate::io::get_str_from_cstr;

use super::{
    bindings::{fs, EINVAL, EISDIR},
    dentry::{raw_inode_clone, Dentry, DentryInner},
    inode::{Inode, InodeData},
    s_isblk, s_ischr, s_isdir, s_isreg, DevId,
};

fn into_slice<'a>(buf: *const u8, bufsize: &usize) -> &'a [u8] {
    unsafe { core::slice::from_raw_parts(buf, *bufsize) }
}

fn into_mut_slice<'a>(buf: *mut u8, bufsize: &usize) -> &'a mut [u8] {
    unsafe { core::slice::from_raw_parts_mut(buf, *bufsize) }
}

#[no_mangle]
pub extern "C" fn fs_mount(
    mountpoint: *mut DentryInner, // borrowed
    source: *const c_char,
    mountpoint_str: *const c_char,
    fstype: *const c_char,
    flags: u64,
    _data: *const c_void,
) -> i32 {
    let mountpoint = Dentry::from_raw(mountpoint).unwrap();

    assert_ne!(mountpoint.flags & D_PRESENT, 0);

    let source = get_str_from_cstr(source).unwrap();
    let mountpoint_str = get_str_from_cstr(mountpoint_str).unwrap();
    let fstype = get_str_from_cstr(fstype).unwrap();

    // TODO: data
    match super::mount::do_mount(
        mountpoint,
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
    file: Arc<dyn Inode>,
    buffer: &mut [u8],
    offset: usize,
) -> KResult<usize> {
    let mode = { file.idata().lock().mode };

    match mode {
        mode if s_isdir(mode) => Err(EISDIR),
        mode if s_isreg(mode) => file.read(buffer, offset),
        mode if s_isblk(mode) => {
            let device = BlockDevice::get(file.devid()?)?;
            let mut buffer = ByteBuffer::new(buffer);

            Ok(device.read_some(offset, &mut buffer)?.allow_partial())
        }
        mode if s_ischr(mode) => {
            let devid = file.devid()?;

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

fn do_write(
    file: Arc<dyn Inode>,
    buffer: &[u8],
    offset: usize,
) -> KResult<usize> {
    let mode = { file.idata().lock().mode };

    match mode {
        mode if s_isdir(mode) => Err(EISDIR),
        mode if s_isreg(mode) => file.write(buffer, offset),
        mode if s_isblk(mode) => Err(EINVAL), // TODO
        mode if s_ischr(mode) => {
            let devid = file.devid()?;

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

#[no_mangle]
pub extern "C" fn fs_read(
    file: *const *const dyn Inode, // borrowed
    buf: *mut u8,
    bufsize: usize,
    offset: usize,
    n: usize,
) -> isize {
    let file = raw_inode_clone(file);

    let bufsize = bufsize.min(n);
    let buffer = into_mut_slice(buf, &bufsize);

    match do_read(file, buffer, offset) {
        Ok(n) => n as isize,
        Err(e) => -(e as isize),
    }
}

#[no_mangle]
pub extern "C" fn fs_write(
    file: *const *const dyn Inode, // borrowed
    buf: *const u8,
    offset: usize,
    n: usize,
) -> isize {
    let file = raw_inode_clone(file);
    let buffer = into_slice(buf, &n);

    match do_write(file, buffer, offset) {
        Ok(n) => n as isize,
        Err(e) => -(e as isize),
    }
}

#[no_mangle]
pub extern "C" fn fs_statx(
    file: *const *const dyn Inode, // borrowed
    stat: *mut statx,
    mask: u32,
) -> i32 {
    let file = raw_inode_clone(file);
    let statx = unsafe { stat.as_mut() }.unwrap();

    match file.statx(statx, mask) {
        Ok(_) => 0,
        Err(e) => -(e as i32),
    }
}

#[no_mangle]
pub extern "C" fn fs_truncate(
    file: *const *const dyn Inode, // borrowed
    size: usize,
) -> i32 {
    let file = raw_inode_clone(file);

    match file.truncate(size) {
        Ok(_) => 0,
        Err(e) => -(e as i32),
    }
}

#[no_mangle]
pub extern "C" fn fs_readlink(
    file: *const *const dyn Inode, // borrowed
    buf: *mut c_char,
    bufsize: usize,
) -> i32 {
    let file = raw_inode_clone(file);
    let buffer = into_mut_slice(buf as *mut u8, &bufsize);

    match file.readlink(buffer) {
        Ok(n) => n as i32,
        Err(e) => -(e as i32),
    }
}

#[no_mangle]
pub extern "C" fn fs_creat(
    at: *mut DentryInner, // borrowed
    mode: mode_t,
) -> i32 {
    let parent = Dentry::parent_from_raw(at).unwrap();
    let mut at = Dentry::from_raw(at).unwrap();

    assert_ne!(parent.flags & D_PRESENT, 0);
    assert_eq!(at.flags & D_PRESENT, 0);

    match parent.get_inode_clone().creat(&mut at, mode as u32) {
        Ok(_) => 0,
        Err(e) => -(e as i32),
    }
}

#[no_mangle]
pub extern "C" fn fs_mkdir(
    at: *mut DentryInner, // borrowed
    mode: mode_t,
) -> i32 {
    let parent = Dentry::parent_from_raw(at).unwrap();
    let mut at = Dentry::from_raw(at).unwrap();

    assert_ne!(parent.flags & D_PRESENT, 0);
    assert_eq!(at.flags & D_PRESENT, 0);

    match parent.get_inode_clone().mkdir(&mut at, mode as u32) {
        Ok(_) => 0,
        Err(e) => -(e as i32),
    }
}

#[no_mangle]
pub extern "C" fn fs_mknod(
    at: *mut DentryInner, // borrowed
    mode: mode_t,
    dev: dev_t,
) -> i32 {
    let parent = Dentry::parent_from_raw(at).unwrap();
    let mut at = Dentry::from_raw(at).unwrap();

    assert_ne!(parent.flags & D_PRESENT, 0);
    assert_eq!(at.flags & D_PRESENT, 0);

    match parent
        .get_inode_clone()
        .mknod(&mut at, mode as u32, dev as DevId)
    {
        Ok(_) => 0,
        Err(e) => -(e as i32),
    }
}

#[no_mangle]
pub extern "C" fn fs_symlink(
    at: *mut DentryInner, // borrowed
    target: *const c_char,
) -> i32 {
    let parent = Dentry::parent_from_raw(at).unwrap();
    let mut at = Dentry::from_raw(at).unwrap();

    assert_ne!(parent.flags & D_PRESENT, 0);
    assert_eq!(at.flags & D_PRESENT, 0);

    match parent
        .get_inode_clone()
        .symlink(&mut at, get_str_from_cstr(target).unwrap())
    {
        Ok(_) => 0,
        Err(e) => -(e as i32),
    }
}

#[no_mangle]
pub extern "C" fn fs_unlink(at: *mut DentryInner, // borrowed
) -> i32 {
    let parent = Dentry::parent_from_raw(at).unwrap();
    let mut at = Dentry::from_raw(at).unwrap();

    assert_ne!(parent.flags & D_PRESENT, 0);
    assert_ne!(at.flags & D_PRESENT, 0);

    match parent.get_inode_clone().unlink(&mut at) {
        Ok(_) => 0,
        Err(e) => -(e as i32),
    }
}

#[no_mangle]
pub extern "C" fn r_dentry_save_inode(
    dent: *mut DentryInner,         // borrowed
    inode: *const *const dyn Inode, // borrowed
) {
    let mut dent = Dentry::from_raw(dent).unwrap();
    let inode = raw_inode_clone(inode);

    dent.save_inode(inode);
}

#[no_mangle]
pub extern "C" fn r_get_inode_mode(
    inode: *const *const dyn Inode, // borrowed
) -> mode_t {
    let inode = raw_inode_clone(inode);
    let idata = inode.idata().lock();

    idata.mode as _
}

#[no_mangle]
pub extern "C" fn r_get_inode_size(
    inode: *const *const dyn Inode, // borrowed
) -> mode_t {
    let inode = raw_inode_clone(inode);
    let idata = inode.idata().lock();

    idata.size as _
}

extern "C" {
    fn call_callback(
        callback: *const c_void,
        filename: *const c_char,
        filename_len: usize,
        inode: *const *const dyn Inode,
        idata: *const InodeData,
        always_zero: u8,
    ) -> i32;
}

#[no_mangle]
pub extern "C" fn fs_readdir(
    file: *const *const dyn Inode, // borrowed
    offset: usize,
    callback: *const c_void,
) -> i64 {
    let inode = raw_inode_clone(file);

    let ret = inode.readdir(
        offset,
        &mut move |filename: &str,
                   ind: &Arc<dyn Inode>,
                   idata: &InodeData,
                   zero: u8| {
            // TODO!!!: CHANGE THIS
            let handle = Arc::into_raw(ind.clone());

            let ret = unsafe {
                call_callback(
                    callback,
                    filename.as_ptr() as *const c_char,
                    filename.len(),
                    &handle,
                    idata,
                    zero,
                )
            };

            unsafe { Arc::from_raw(handle) };

            Ok(ret)
        },
    );

    match ret {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
