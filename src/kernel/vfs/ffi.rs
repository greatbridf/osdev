use crate::io::RawBuffer;

use super::{dentry::Dentry, inode::Inode};

#[no_mangle]
pub extern "C" fn fs_read(
    file: *const Dentry, // borrowed
    buf: *mut u8,
    bufsize: usize,
    offset: usize,
    n: usize,
) -> isize {
    let file = Dentry::from_raw(&file);

    let bufsize = bufsize.min(n);
    let mut buffer = RawBuffer::new_from_raw(buf, bufsize);

    match file.read(&mut buffer, offset) {
        Ok(n) => n as isize,
        Err(e) => -(e as isize),
    }
}
