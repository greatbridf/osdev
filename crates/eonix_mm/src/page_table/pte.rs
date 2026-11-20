use core::marker::PhantomData;
use core::ops::Deref;
use core::sync::atomic::{AtomicUsize, Ordering};

use bitflags::bitflags;

use crate::arch::RawPTE;
use crate::paging::PFN;

pub struct Any;
pub struct Table;
pub struct Leaf;

#[repr(transparent)]
pub struct PTEntry<K: ?Sized = Any> {
    value: AtomicUsize,
    _kind: PhantomData<K>,
}

pub type PTELeaf = PTEntry<Leaf>;
pub type PTETable = PTEntry<Table>;

bitflags! {
    #[derive(Clone, Copy, PartialEq)]
    pub struct TableAttribute: usize {
        const PRESENT = 1;
        const USER = 2;
        const ACCESSED = 4;
        const GLOBAL = 8;
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct PageAttribute: usize {
        const PRESENT = 1;
        const READ = 2;
        const WRITE = 4;
        const EXECUTE = 8;
        const USER = 16;
        const ACCESSED = 32;
        const DIRTY = 64;
        const GLOBAL = 128;
        const COPY_ON_WRITE = 256;
        const MAPPED = 512;
        const ANONYMOUS = 1024;
        const HUGE = 2048;
    }
}

impl PTEntry {
    #[inline(always)]
    pub fn _conv<K>(&self) -> &PTEntry<K> {
        unsafe {
            // SAFETY: All PTEntry<K> have the same layout and alignment.
            &*(self as *const _ as *const PTEntry<K>)
        }
    }

    #[inline(always)]
    pub fn as_table(&self) -> &PTEntry<Table> {
        self._conv()
    }

    #[inline(always)]
    pub fn as_leaf(&self) -> &PTEntry<Leaf> {
        self._conv()
    }

    #[inline(always)]
    pub fn set_raw(&self, pte: RawPTE) {
        self.value.store(pte.into_raw(), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn get_raw(&self) -> RawPTE {
        RawPTE::from_raw(self.value.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn pfn(&self) -> PFN {
        let pte = self.get_raw();
        pte.get_pfn()
    }

    #[inline(always)]
    pub fn clear(&self) {
        self.set_raw(RawPTE::null());
    }
}

impl PTEntry<Table> {
    #[inline(always)]
    pub fn get(&self) -> Option<(PFN, TableAttribute)> {
        let pte = self.get_raw();
        let attr = pte.get_table()?;
        Some((pte.get_pfn(), attr))
    }

    #[inline(always)]
    pub fn set(&self, pfn: PFN, attr: TableAttribute) {
        let mut pte = RawPTE::null();
        pte.set_table(pfn, attr);
        self.set_raw(pte)
    }

    #[inline(always)]
    pub fn attr(&self) -> Option<TableAttribute> {
        self.get().map(|(_, attr)| attr)
    }
}

impl PTEntry<Leaf> {
    #[inline(always)]
    pub fn get(&self) -> Option<(PFN, PageAttribute)> {
        let pte = self.get_raw();
        let attr = pte.get_leaf()?;
        Some((pte.get_pfn(), attr))
    }

    #[inline(always)]
    pub fn set(&self, pfn: PFN, attr: PageAttribute) {
        let mut pte = RawPTE::null();
        pte.set_leaf(pfn, attr);
        self.set_raw(pte)
    }

    #[inline(always)]
    pub fn attr(&self) -> Option<PageAttribute> {
        self.get().map(|(_, attr)| attr)
    }
}

trait Type {}

impl Type for Table {}
impl Type for Leaf {}

impl<K: Type> Deref for PTEntry<K> {
    type Target = PTEntry;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe {
            // SAFETY: K is zero sized so they have the same alignment.
            &*(self as *const _ as *const Self::Target)
        }
    }
}

pub trait PTE: Sized {
    fn into_raw(self) -> usize;
    fn from_raw(val: usize) -> Self;
    fn null() -> Self;

    fn get_pfn(&self) -> PFN;
    fn get_leaf(&self) -> Option<PageAttribute>;
    fn get_table(&self) -> Option<TableAttribute>;

    fn set_leaf(&mut self, pfn: PFN, attr: PageAttribute);
    fn set_table(&mut self, pfn: PFN, attr: TableAttribute);
}
