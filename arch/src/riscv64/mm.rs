use core::{marker::PhantomData, ptr::NonNull};
use eonix_mm::{
    address::{Addr as _, PAddr},
    page_table::{PageAttribute, PageTableLevel, PagingMode, RawPageTable, PTE},
    paging::{PageBlock, PFN},
};

pub const PAGE_SIZE: usize = 0x1000;
const PAGE_TABLE_BASE: PFN = PFN::from_val(0x8030_0000 >> 12);

const PA_V: u64 = 0b1 << 0;
const PA_R: u64 = 0b1 << 1;
const PA_W: u64 = 0b1 << 2;
const PA_X: u64 = 0b1 << 3;
const PA_U: u64 = 0b1 << 4;
const PA_G: u64 = 0b1 << 5;
const PA_A: u64 = 0b1 << 6;
const PA_D: u64 = 0b1 << 7;

// in RSW
const PA_COW: u64 = 0b1 << 8;
const PA_MMAP: u64 = 0b1 << 9;

const PA_SHIFT: u64 = 10;
const PA_MASK: u64 = 0xFFC0_0000_0000_03FF; // 44 bit PPN, from 10 to 53
// Bit 0-9 (V, R, W, X, U, G, A, D, RSW)
const PA_FLAGS_MASK: u64 = 0x3FF; // 0b11_1111_1111



#[repr(transparent)]
pub struct PTE64(u64);

#[derive(Clone, Copy)]
pub struct PageAttribute64(u64);

pub struct RawPageTableSv39<'a>(NonNull<PTE64>, PhantomData<&'a ()>);

pub struct PagingModeSv39;

impl PTE for PTE64 {
    type Attr = PageAttribute64;

    fn set(&mut self, pfn: PFN, attr: Self::Attr) {
        self.0 = (PAddr::from(pfn).addr() as u64 & !PA_MASK) | (attr.0 & PA_MASK);
    }

    fn get(&self) -> (PFN, Self::Attr) {
        (
            PFN::from(PAddr::from((self.0 & !PA_MASK) as usize)),
            PageAttribute64(self.0 & PA_MASK),
        )
    }

    fn take(&mut self) -> (PFN, Self::Attr) {
        let pfn_attr = self.get();
        self.0 = 0;
        pfn_attr
    }
}

impl PagingMode for PagingModeSv39 {
    type Entry = PTE64;
    type RawTable<'a> = RawPageTableSv39<'a>;
    const LEVELS: &'static [PageTableLevel] = &[
        PageTableLevel::new(30, 9),
        PageTableLevel::new(21, 9),
        PageTableLevel::new(12, 9),
    ];
    const KERNEL_ROOT_TABLE_PFN: PFN = PAGE_TABLE_BASE;
}

impl<'a> RawPageTable<'a> for RawPageTableSv39<'a> {
    type Entry = PTE64;

    fn index(&self, index: u16) -> &'a Self::Entry {
        unsafe { &self.0.cast::<[PTE64; 512]>().as_ref()[index as usize] }
    }

    fn index_mut(&mut self, index: u16) -> &'a mut Self::Entry {
        unsafe { &mut self.0.cast::<[PTE64; 512]>().as_mut()[index as usize] }
    }

    unsafe fn from_ptr(ptr: NonNull<PageBlock>) -> Self {
        Self(ptr.cast(), PhantomData)
    }
}

impl PageAttribute for PageAttribute64 {
    fn new() -> Self {
        Self(PA_R)
    }

    fn present(self, present: bool) -> Self {
        if present {
            Self(self.0 | PA_V)
        } else {
            Self(self.0 & !PA_V)
        }
    }

    fn write(self, write: bool) -> Self {
        if write {
            Self(self.0 | PA_W)
        } else {
            Self(self.0 & !PA_W)
        }
    }

    fn execute(self, execute: bool) -> Self {
        if execute {
            Self(self.0 | PA_X)
        } else {
            Self(self.0 & !PA_X)
        }
    }

    fn user(self, user: bool) -> Self {
        if user {
            Self(self.0 | PA_U)
        } else {
            Self(self.0 & !PA_U)
        }
    }

    fn accessed(self, accessed: bool) -> Self {
        if accessed {
            Self(self.0 | PA_A)
        } else {
            Self(self.0 & !PA_A)
        }
    }

    fn dirty(self, dirty: bool) -> Self {
        if dirty {
            Self(self.0 | PA_D)
        } else {
            Self(self.0 & !PA_D)
        }
    }

    fn global(self, global: bool) -> Self {
        if global {
            Self(self.0 | PA_G)
        } else {
            Self(self.0 & !PA_G)
        }
    }

    fn copy_on_write(self, cow: bool) -> Self {
        if cow {
            Self(self.0 | PA_COW)
        } else {
            Self(self.0 & !PA_COW)
        }
    }

    fn mapped(self, mmap: bool) -> Self {
        if mmap {
            Self(self.0 | PA_MMAP)
        } else {
            Self(self.0 & !PA_MMAP)
        }
    }

    fn anonymous(self, anon: bool) -> Self {
        todo!()
    }

    fn is_present(&self) -> bool {
        self.0 & PA_V != 0
    }

    fn is_write(&self) -> bool {
        self.0 & PA_W != 0
    }

    fn is_execute(&self) -> bool {
        self.0 & PA_X != 0
    }

    fn is_user(&self) -> bool {
        self.0 & PA_U != 0
    }

    fn is_accessed(&self) -> bool {
        self.0 & PA_A != 0
    }

    fn is_dirty(&self) -> bool {
        self.0 & PA_D != 0
    }

    fn is_global(&self) -> bool {
        self.0 & PA_G != 0
    }

    fn is_copy_on_write(&self) -> bool {
        self.0 & PA_COW != 0
    }

    fn is_mapped(&self) -> bool {
        self.0 & PA_MMAP != 0
    }

    fn is_anonymous(&self) -> bool {
        todo!()
    }
}

pub type DefaultPagingMode = PagingModeSv39;
