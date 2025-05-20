use core::{marker::PhantomData, ptr::NonNull};
use eonix_mm::{
    address::{Addr as _, PAddr},
    page_table::{PageAttribute, PageTableLevel, PagingMode, RawPageTable, PTE},
    paging::{PageBlock, PFN},
};

pub const PAGE_SIZE: usize = 0x1000;

const KERNEL_PML4_PFN: PFN = PFN::from_val(0x2000 >> 12);

const PA_P: u64 = 0x001;
const PA_RW: u64 = 0x002;
const PA_US: u64 = 0x004;
#[allow(dead_code)]
const PA_PWT: u64 = 0x008;
#[allow(dead_code)]
const PA_PCD: u64 = 0x010;
const PA_A: u64 = 0x020;
const PA_D: u64 = 0x040;
#[allow(dead_code)]
const PA_PS: u64 = 0x080;
const PA_G: u64 = 0x100;
const PA_COW: u64 = 0x200;
const PA_MMAP: u64 = 0x400;
const PA_ANON: u64 = 0x800;
const PA_NXE: u64 = 0x8000_0000_0000_0000;
const PA_MASK: u64 = 0xfff0_0000_0000_0fff;

#[repr(transparent)]
pub struct PTE64(u64);

#[derive(Clone, Copy)]
pub struct PageAttribute64(u64);

pub struct RawPageTable4Levels<'a>(NonNull<PTE64>, PhantomData<&'a ()>);

pub struct PagingMode4Levels;

impl PTE for PTE64 {
    type Attr = PageAttribute64;

    fn set(&mut self, pfn: PFN, attr: Self::Attr) {
        let paddr = PAddr::from(pfn).addr();

        self.0 = (paddr as u64 & !PA_MASK) | (attr.0 & PA_MASK);
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

impl PagingMode for PagingMode4Levels {
    type Entry = PTE64;
    type RawTable<'a> = RawPageTable4Levels<'a>;

    const LEVELS: &'static [PageTableLevel] = &[
        PageTableLevel::new(39, 9),
        PageTableLevel::new(30, 9),
        PageTableLevel::new(21, 9),
        PageTableLevel::new(12, 9),
    ];

    const KERNEL_ROOT_TABLE_PFN: PFN = KERNEL_PML4_PFN;
}

impl<'a> RawPageTable<'a> for RawPageTable4Levels<'a> {
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
        Self(PA_NXE)
    }

    fn present(self, present: bool) -> Self {
        if present {
            Self(self.0 | PA_P)
        } else {
            Self(self.0 & !PA_P)
        }
    }

    fn write(self, write: bool) -> Self {
        if write {
            Self(self.0 | PA_RW)
        } else {
            Self(self.0 & !PA_RW)
        }
    }

    fn execute(self, execute: bool) -> Self {
        if execute {
            Self(self.0 & !PA_NXE)
        } else {
            Self(self.0 | PA_NXE)
        }
    }

    fn user(self, user: bool) -> Self {
        if user {
            Self(self.0 | PA_US)
        } else {
            Self(self.0 & !PA_US)
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
        if anon {
            Self(self.0 | PA_ANON)
        } else {
            Self(self.0 & !PA_ANON)
        }
    }

    fn is_present(&self) -> bool {
        self.0 & PA_P != 0
    }

    fn is_write(&self) -> bool {
        self.0 & PA_RW != 0
    }

    fn is_execute(&self) -> bool {
        self.0 & PA_NXE == 0
    }

    fn is_user(&self) -> bool {
        self.0 & PA_US != 0
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
        self.0 & PA_ANON != 0
    }
}

pub type DefaultPagingMode = PagingMode4Levels;
