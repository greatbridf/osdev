use crate::prelude::*;
use bindings::EFAULT;
use core::{cmp, mem::MaybeUninit};

#[must_use]
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

    #[must_use]
    fn fill(&mut self, data: &[u8]) -> KResult<FillResult>;

    fn available(&self) -> usize {
        self.total() - self.wrote()
    }

    fn get_writer(&mut self) -> BufferWrite<'_, Self>
    where
        Self: Sized,
    {
        BufferWrite(self)
    }
}

pub trait BufferFill<T: Copy> {
    fn copy(&mut self, object: &T) -> KResult<FillResult>;
}

impl<T: Copy, B: Buffer + ?Sized> BufferFill<T> for B {
    fn copy(&mut self, object: &T) -> KResult<FillResult> {
        let ptr = object as *const T as *const u8;
        let len = core::mem::size_of::<T>();

        // SAFETY: `object` is a valid object.
        self.fill(unsafe { core::slice::from_raw_parts(ptr, len) })
    }
}

pub struct BufferWrite<'b, B>(&'b mut B)
where
    B: Buffer + ?Sized;

impl<'b, B> core::fmt::Write for BufferWrite<'b, B>
where
    B: Buffer + ?Sized,
{
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        match self.0.fill(s.as_bytes()) {
            Ok(FillResult::Done(_)) => Ok(()),
            _ => Err(core::fmt::Error),
        }
    }
}

pub struct UninitBuffer<'lt, T: Copy + Sized> {
    data: Box<MaybeUninit<T>>,
    buffer: ByteBuffer<'lt>,
}

impl<'lt, T: Copy + Sized> UninitBuffer<'lt, T> {
    pub fn new() -> Self {
        let mut data = Box::new(MaybeUninit::uninit());
        let ptr = data.as_mut_ptr();

        Self {
            data,
            buffer: ByteBuffer::from(unsafe {
                core::slice::from_raw_parts_mut(ptr as *mut u8, core::mem::size_of::<T>())
            }),
        }
    }

    pub fn assume_filled_ref(&self) -> KResult<&T> {
        if self.buffer.available() != 0 {
            Err(EFAULT)
        } else {
            Ok(unsafe { self.data.assume_init_ref() })
        }
    }

    pub fn assume_init(self) -> KResult<T> {
        if self.buffer.available() != 0 {
            Err(EFAULT)
        } else {
            Ok(unsafe { *self.data.assume_init() })
        }
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

impl<'lt, T: Copy + Sized> From<&'lt mut [T]> for ByteBuffer<'lt> {
    fn from(value: &'lt mut [T]) -> Self {
        Self {
            buf: unsafe {
                core::slice::from_raw_parts_mut(
                    value.as_ptr() as *mut u8,
                    core::mem::size_of::<T>() * value.len(),
                )
            },
            cur: 0,
        }
    }
}

impl Buffer for ByteBuffer<'_> {
    fn total(&self) -> usize {
        self.buf.len()
    }

    fn fill(&mut self, data: &[u8]) -> KResult<FillResult> {
        match self.available() {
            0 => Ok(FillResult::Full),
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

/// Iterator that generates chunks of a given length from a start index
/// until the end of the total length.
///
/// The iterator returns a tuple of (start, len) for each chunk.
pub struct Chunks {
    start: usize,
    end: usize,
    cur: usize,
    chunk_len: usize,
}

impl Chunks {
    pub const fn new(start: usize, total_len: usize, chunk_len: usize) -> Self {
        Self {
            start,
            end: start + total_len,
            cur: start,
            chunk_len,
        }
    }
}

impl Iterator for Chunks {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        if self.cur >= self.end {
            return None;
        }

        let start = self.cur;
        let len = cmp::min(self.chunk_len, self.end - start);

        self.cur += self.chunk_len;
        Some((start, len))
    }
}
