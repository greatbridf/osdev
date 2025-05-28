use core::{marker::PhantomData, ptr::NonNull};
use eonix_mm::{
    address::{Addr as _, PAddr},
    page_table::{
        PageAttribute, PageTableLevel, PagingMode, RawAttribute, RawPageTable, TableAttribute, PTE,
    },
    paging::{PageBlock, PFN},
};

pub const PAGE_SIZE: usize = 0x1000;

const KERNEL_PML4_PFN: PFN = PFN::from_val(0x1000 >> 12);

const PA_P: u64 = 0x001;
const PA_RW: u64 = 0x002;
const PA_US: u64 = 0x004;
#[allow(dead_code)]
const PA_PWT: u64 = 0x008;
#[allow(dead_code)]
const PA_PCD: u64 = 0x010;
const PA_A: u64 = 0x020;
const PA_D: u64 = 0x040;
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

impl RawAttribute for PageAttribute64 {
    fn null() -> Self {
        Self(0)
    }

    fn as_table_attr(self) -> Option<TableAttribute> {
        let mut table_attr = TableAttribute::empty();

        if self.0 & PA_PS != 0 {
            panic!("Encountered a huge page while parsing table attributes");
        }

        if self.0 & PA_P != 0 {
            table_attr |= TableAttribute::PRESENT;
        }
        if self.0 & PA_G != 0 {
            table_attr |= TableAttribute::GLOBAL;
        }
        if self.0 & PA_US != 0 {
            table_attr |= TableAttribute::USER;
        }
        if self.0 & PA_A != 0 {
            table_attr |= TableAttribute::ACCESSED;
        }

        Some(table_attr)
    }

    fn as_page_attr(self) -> Option<PageAttribute> {
        let mut page_attr = PageAttribute::READ;

        if self.0 & PA_P != 0 {
            page_attr |= PageAttribute::PRESENT;
        }

        if self.0 & PA_RW != 0 {
            page_attr |= PageAttribute::WRITE;
        }

        if self.0 & PA_NXE == 0 {
            page_attr |= PageAttribute::EXECUTE;
        }

        if self.0 & PA_US != 0 {
            page_attr |= PageAttribute::USER;
        }

        if self.0 & PA_A != 0 {
            page_attr |= PageAttribute::ACCESSED;
        }

        if self.0 & PA_D != 0 {
            page_attr |= PageAttribute::DIRTY;
        }

        if self.0 & PA_G != 0 {
            page_attr |= PageAttribute::GLOBAL;
        }

        if self.0 & PA_COW != 0 {
            page_attr |= PageAttribute::COPY_ON_WRITE;
        }

        if self.0 & PA_MMAP != 0 {
            page_attr |= PageAttribute::MAPPED;
        }

        if self.0 & PA_ANON != 0 {
            page_attr |= PageAttribute::ANONYMOUS;
        }

        if self.0 & PA_PS != 0 {
            page_attr |= PageAttribute::HUGE;
        }

        Some(page_attr)
    }
}

impl From<PageAttribute> for PageAttribute64 {
    fn from(page_attr: PageAttribute) -> Self {
        let mut raw_attr = PA_NXE;

        for attr in page_attr.iter() {
            match attr {
                PageAttribute::PRESENT => raw_attr |= PA_P,
                PageAttribute::READ => {}
                PageAttribute::WRITE => raw_attr |= PA_RW,
                PageAttribute::EXECUTE => raw_attr &= !PA_NXE,
                PageAttribute::USER => raw_attr |= PA_US,
                PageAttribute::ACCESSED => raw_attr |= PA_A,
                PageAttribute::DIRTY => raw_attr |= PA_D,
                PageAttribute::GLOBAL => raw_attr |= PA_G,
                PageAttribute::COPY_ON_WRITE => raw_attr |= PA_COW,
                PageAttribute::MAPPED => raw_attr |= PA_MMAP,
                PageAttribute::ANONYMOUS => raw_attr |= PA_ANON,
                PageAttribute::HUGE => raw_attr |= PA_PS,
                _ => unreachable!("Invalid page attribute"),
            }
        }

        Self(raw_attr)
    }
}

impl From<TableAttribute> for PageAttribute64 {
    fn from(table_attr: TableAttribute) -> Self {
        let mut raw_attr = PA_RW;

        for attr in table_attr.iter() {
            match attr {
                TableAttribute::PRESENT => raw_attr |= PA_P,
                TableAttribute::GLOBAL => raw_attr |= PA_G,
                TableAttribute::USER => raw_attr |= PA_US,
                TableAttribute::ACCESSED => raw_attr |= PA_A,
                _ => unreachable!("Invalid table attribute"),
            }
        }

        Self(raw_attr)
    }
}

pub type DefaultPagingMode = PagingMode4Levels;
