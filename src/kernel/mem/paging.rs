use eonix_mm::paging::Folio as _;

use super::folio::FolioOwned;
use crate::io::{Buffer, FillResult};

/// A buffer that wraps a page and provides a `Buffer` interface.
pub struct PageBuffer {
    page: FolioOwned,
    offset: usize,
}

pub trait AllocZeroed {
    fn zeroed() -> Self;
}

impl PageBuffer {
    pub fn new() -> Self {
        Self {
            page: FolioOwned::alloc(),
            offset: 0,
        }
    }

    pub fn all(&self) -> &[u8] {
        self.page.as_bytes()
    }

    pub fn data(&self) -> &[u8] {
        &self.all()[..self.offset]
    }

    pub fn available_mut(&mut self) -> &mut [u8] {
        &mut self.page.as_bytes_mut()[self.offset..]
    }
}

impl Buffer for PageBuffer {
    fn total(&self) -> usize {
        self.page.len()
    }

    fn wrote(&self) -> usize {
        self.offset
    }

    fn fill(&mut self, data: &[u8]) -> crate::KResult<crate::io::FillResult> {
        let available = self.available_mut();
        if available.len() == 0 {
            return Ok(FillResult::Full);
        }

        let len = core::cmp::min(data.len(), available.len());
        available[..len].copy_from_slice(&data[..len]);
        self.offset += len;

        if len < data.len() {
            Ok(FillResult::Partial(len))
        } else {
            Ok(FillResult::Done(len))
        }
    }
}
