use bindings::EFAULT;

use crate::prelude::*;
use core::ffi::{c_char, c_size_t, c_uchar};

pub struct Buffer {
    buf: *mut c_uchar,
    size: usize,
    rem: usize,
}

impl Buffer {
    pub fn new(buf: *mut c_uchar, _size: c_size_t) -> Self {
        let size = _size as usize;
        Self {
            buf,
            size,
            rem: size,
        }
    }

    pub fn count(&self) -> usize {
        self.size - self.rem
    }
}

use core::fmt::Write;
impl Write for Buffer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let s = s.as_bytes();
        let len = s.len();

        if self.rem <= len {
            return Err(core::fmt::Error);
        }

        unsafe {
            core::ptr::copy_nonoverlapping(s.as_ptr(), self.buf, len);
            self.buf = self.buf.add(len);
        }
        self.rem -= len;

        Ok(())
    }
}

pub fn get_str_from_cstr<'a>(cstr: *const c_char) -> KResult<&'a str> {
    if cstr.is_null() {
        return Err(EFAULT);
    }

    let cstr = unsafe { core::ffi::CStr::from_ptr::<'a>(cstr) };
    cstr.to_str().map_err(|_| EFAULT)
}

pub fn get_cxx_std_string<'a>(
    cxx_string: &'a bindings::std::string,
) -> KResult<&'a str> {
    let arr: &'a [u8] = unsafe {
        let mut result = bindings::rust_get_cxx_string_result {
            data: core::ptr::null(),
            len: 0,
        };

        bindings::rust_get_cxx_string(
            cxx_string.as_ptr() as _,
            &raw mut result,
        );

        core::slice::from_raw_parts(result.data as *const u8, result.len)
    };

    core::str::from_utf8(arr).map_err(|_| EFAULT)
}

pub fn operator_eql_cxx_std_string(
    lhs: &mut bindings::std::string,
    rhs: &bindings::std::string,
) {
    unsafe {
        bindings::rust_operator_eql_cxx_string(
            rhs.as_ptr() as _,
            lhs.as_ptr() as _,
        )
    };
}

/// Copy data from src to dst, starting from offset, and copy at most count bytes.
///
/// # Return
///
/// The number of bytes copied.
pub fn copy_offset_count(
    src: &[u8],
    dst: &mut [u8],
    offset: usize,
    count: usize,
) -> usize {
    if offset >= src.len() {
        return 0;
    }

    let count = {
        let count = count.min(dst.len());

        if offset + count > src.len() {
            src.len() - offset
        } else {
            count
        }
    };

    dst[..count].copy_from_slice(&src[offset..offset + count]);

    count
}
