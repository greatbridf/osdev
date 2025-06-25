use crate::kernel::constants::EFAULT;
use crate::prelude::*;
use core::{cmp, mem::MaybeUninit};

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

pub trait Stream {
    fn poll_data<'a>(&mut self, buf: &'a mut [u8]) -> KResult<Option<&'a mut [u8]>>;
    fn ignore(&mut self, len: usize) -> KResult<Option<usize>>;
}

pub trait IntoStream {
    type Stream: Stream;

    fn into_stream(self) -> Self::Stream;
}

pub trait StreamRead {
    fn read_till_end(
        &mut self,
        buffer: &mut [u8],
        func: impl FnMut(&mut [u8]) -> KResult<()>,
    ) -> KResult<usize>;

    fn ignore_all(&mut self) -> KResult<usize>;
}

impl<T> StreamRead for T
where
    T: Stream + ?Sized,
{
    fn read_till_end(
        &mut self,
        buffer: &mut [u8],
        mut func: impl FnMut(&mut [u8]) -> KResult<()>,
    ) -> KResult<usize> {
        let mut total = 0;
        while let Some(data) = self.poll_data(buffer)? {
            func(data)?;
            total += data.len();
        }
        Ok(total)
    }

    fn ignore_all(&mut self) -> KResult<usize> {
        let mut total = 0;
        while let Some(len) = self.ignore(usize::MAX)? {
            total += len;
        }
        Ok(total)
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
    end: usize,
    cur: usize,
    chunk_len: usize,
}

impl Chunks {
    pub const fn new(start: usize, total_len: usize, chunk_len: usize) -> Self {
        Self {
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

pub struct ByteStream<'a> {
    data: &'a [u8],
    cur: usize,
}

impl<'a> ByteStream<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, cur: 0 }
    }
}

impl<'a> Stream for ByteStream<'a> {
    fn poll_data<'b>(&mut self, buf: &'b mut [u8]) -> KResult<Option<&'b mut [u8]>> {
        if self.cur >= self.data.len() {
            return Ok(None);
        }

        let end = core::cmp::min(self.data.len(), self.cur + buf.len());

        let data = &self.data[self.cur..end];
        let buf = &mut buf[..data.len()];

        buf.copy_from_slice(data);
        self.cur += data.len();

        Ok(Some(buf))
    }

    fn ignore(&mut self, len: usize) -> KResult<Option<usize>> {
        if self.cur >= self.data.len() {
            return Ok(None);
        }

        let remaining = self.data.len() - self.cur;
        let ignored = core::cmp::min(remaining, len);
        self.cur += ignored;

        Ok(Some(ignored))
    }
}

impl<'a> IntoStream for &'a [u8] {
    type Stream = ByteStream<'a>;

    fn into_stream(self) -> Self::Stream {
        ByteStream::new(self)
    }
}
