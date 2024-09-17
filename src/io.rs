use core::ffi::{c_size_t, c_uchar};

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
