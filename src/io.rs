use bindings::EFAULT;

use crate::prelude::*;

use core::{ffi::c_char, fmt::Write, mem::MaybeUninit};

pub enum FillResult {
    Done(usize),
    Partial(usize),
    Full,
}

impl FillResult {
    pub fn ok_or(self, err: u32) -> KResult<()> {
        match self {
            FillResult::Done(_) => Ok(()),
            _ => Err(err),
        }
    }

    pub fn should_stop(self) -> bool {
        return !matches!(self, FillResult::Done(_));
    }

    pub fn allow_partial(self) -> usize {
        match self {
            FillResult::Done(n) | FillResult::Partial(n) => n,
            FillResult::Full => 0,
        }
    }
}

pub trait Buffer {
    fn total(&self) -> usize;
    fn wrote(&self) -> usize;
    fn fill(&mut self, data: &[u8]) -> KResult<FillResult>;
}

pub struct UninitBuffer<'lt, T: Copy + Sized> {
    data: Box<MaybeUninit<T>>,
    buffer: RawBuffer<'lt>,
}

impl<'lt, T: Copy + Sized> UninitBuffer<'lt, T> {
    pub fn new() -> Self {
        let mut data = Box::new(MaybeUninit::uninit());
        let ptr = data.as_mut_ptr();

        Self {
            data,
            buffer: RawBuffer::new_from_slice(unsafe {
                core::slice::from_raw_parts_mut(
                    ptr as *mut u8,
                    core::mem::size_of::<T>(),
                )
            }),
        }
    }

    pub fn assume_filled_ref(&self) -> KResult<&T> {
        if !self.buffer.filled() {
            return Err(EFAULT);
        }

        Ok(unsafe { self.data.assume_init_ref() })
    }
}

impl<'lt, T: Copy + Sized> Buffer for UninitBuffer<'lt, T> {
    fn total(&self) -> usize {
        self.buffer.total()
    }

    fn wrote(&self) -> usize {
        self.buffer.wrote()
    }

    fn fill(&mut self, data: &[u8]) -> KResult<FillResult> {
        self.buffer.fill(data)
    }
}

pub struct RawBuffer<'lt> {
    buf: *mut u8,
    tot: usize,
    cur: usize,
    _phantom: core::marker::PhantomData<&'lt mut u8>,
}

impl<'lt> RawBuffer<'lt> {
    pub fn new_from_mut<T: Copy + Sized>(buf: &'lt mut T) -> Self {
        Self {
            buf: buf as *mut T as *mut u8,
            tot: core::mem::size_of::<T>(),
            cur: 0,
            _phantom: core::marker::PhantomData,
        }
    }

    pub fn new_from_slice<T: Copy + Sized>(buf: &'lt mut [T]) -> Self {
        Self {
            buf: buf.as_mut_ptr() as *mut u8,
            tot: core::mem::size_of::<T>() * buf.len(),
            cur: 0,
            _phantom: core::marker::PhantomData,
        }
    }

    pub fn new_from_raw(buf: &'lt mut *mut u8, tot: usize) -> Self {
        Self {
            buf: *buf,
            tot,
            cur: 0,
            _phantom: core::marker::PhantomData,
        }
    }

    pub fn count(&self) -> usize {
        self.cur
    }

    pub fn total(&self) -> usize {
        self.tot
    }

    pub fn available(&self) -> usize {
        self.total() - self.count()
    }

    pub fn filled(&self) -> bool {
        self.count() == self.total()
    }

    pub fn fill(&mut self, data: &[u8]) -> KResult<FillResult> {
        match self.available() {
            n if n == 0 => Ok(FillResult::Full),
            n if n < data.len() => {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        data.as_ptr(),
                        self.buf.add(self.count()),
                        n,
                    );
                }
                self.cur += n;
                Ok(FillResult::Partial(n))
            }
            _ => {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        data.as_ptr(),
                        self.buf.add(self.count()),
                        data.len(),
                    );
                }
                self.cur += data.len();
                Ok(FillResult::Done(data.len()))
            }
        }
    }
}

impl Buffer for RawBuffer<'_> {
    fn total(&self) -> usize {
        RawBuffer::total(self)
    }

    fn wrote(&self) -> usize {
        self.count()
    }

    fn fill(&mut self, data: &[u8]) -> KResult<FillResult> {
        RawBuffer::fill(self, data)
    }
}

pub struct ByteBuffer<'lt> {
    buf: &'lt mut [u8],
    cur: usize,
}

impl<'lt> ByteBuffer<'lt> {
    pub fn new(buf: &'lt mut [u8]) -> Self {
        Self { buf, cur: 0 }
    }

    pub fn available(&self) -> usize {
        self.buf.len() - self.cur
    }

    pub fn data(&self) -> &[u8] {
        &self.buf[..self.cur]
    }
}

impl Buffer for ByteBuffer<'_> {
    fn total(&self) -> usize {
        self.buf.len()
    }

    fn fill(&mut self, data: &[u8]) -> KResult<FillResult> {
        match self.available() {
            n if n == 0 => Ok(FillResult::Full),
            n if n < data.len() => {
                self.buf[self.cur..].copy_from_slice(&data[..n]);
                self.cur += n;
                Ok(FillResult::Partial(n))
            }
            _ => {
                self.buf[self.cur..self.cur + data.len()].copy_from_slice(data);
                self.cur += data.len();
                Ok(FillResult::Done(data.len()))
            }
        }
    }

    fn wrote(&self) -> usize {
        self.cur
    }
}

impl Write for RawBuffer<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        match self.fill(s.as_bytes()) {
            Ok(FillResult::Done(_)) => Ok(()),
            _ => Err(core::fmt::Error),
        }
    }
}

pub fn get_str_from_cstr<'a>(cstr: *const c_char) -> KResult<&'a str> {
    if cstr.is_null() {
        return Err(EFAULT);
    }

    let cstr = unsafe { core::ffi::CStr::from_ptr::<'a>(cstr) };
    cstr.to_str().map_err(|_| EFAULT)
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
