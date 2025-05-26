use super::{RawPageTable, PTE};
use crate::{
    address::{Addr as _, VAddr},
    paging::PFN,
};

pub trait PagingMode {
    type Entry: PTE;
    type RawTable<'a>: RawPageTable<'a, Entry = Self::Entry>;

    const LEVELS: &'static [PageTableLevel];
    const KERNEL_ROOT_TABLE_PFN: PFN;
}

#[derive(Clone, Copy, PartialOrd, PartialEq)]
pub struct PageTableLevel(usize, usize);

impl PageTableLevel {
    pub const fn new(nth_bit: usize, len: usize) -> Self {
        Self(nth_bit, len)
    }

    pub const fn nth_bit(self) -> usize {
        self.0
    }

    pub const fn len(self) -> usize {
        self.1
    }

    pub const fn page_size(self) -> usize {
        1 << self.nth_bit()
    }

    pub const fn max_index(self) -> u16 {
        (1 << self.len()) - 1
    }

    pub fn index_of(self, vaddr: VAddr) -> u16 {
        ((vaddr.addr() >> self.nth_bit()) & ((1 << self.len()) - 1)) as u16
    }
}
