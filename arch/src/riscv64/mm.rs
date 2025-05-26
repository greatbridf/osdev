use core::{marker::PhantomData, ptr::NonNull};
use eonix_mm::{
    address::{Addr as _, PAddr},
    page_table::{
        PageAttribute, PageTableLevel, PagingMode, RawAttribute, RawPageTable, TableAttribute, PTE,
    },
    paging::{PageBlock, PFN},
};

pub const ROOT_PAGE_TABLE_PHYS_ADDR: usize = 0x8040_0000;
pub const PAGE_TABLE_END: usize = 0x8080_0000;
pub const KIMAGE_PHYS_BASE: usize = 0x8020_0000;
pub const KIMAGE_VIRT_BASE: usize = 0xFFFF_FFFF_FFC0_0000;
pub const PAGE_SIZE: usize = 0x1000;
const PAGE_TABLE_BASE: PFN = PFN::from_val(ROOT_PAGE_TABLE_PHYS_ADDR >> 12);

pub const PA_V: u64 = 0b1 << 0;
pub const PA_R: u64 = 0b1 << 1;
pub const PA_W: u64 = 0b1 << 2;
pub const PA_X: u64 = 0b1 << 3;
pub const PA_U: u64 = 0b1 << 4;
pub const PA_G: u64 = 0b1 << 5;
pub const PA_A: u64 = 0b1 << 6;
pub const PA_D: u64 = 0b1 << 7;

// in RSW
pub const PA_COW: u64 = 0b1 << 8;
pub const PA_MMAP: u64 = 0b1 << 9;

#[allow(dead_code)]
pub const PA_SHIFT: u64 = 10;
pub const PA_MASK: u64 = 0xFFC0_0000_0000_03FF; // 44 bit PPN, from 10 to 53
// Bit 0-9 (V, R, W, X, U, G, A, D, RSW)
#[allow(dead_code)]
pub const PA_FLAGS_MASK: u64 = 0x3FF; // 0b11_1111_1111

pub const PA_KERNEL_RWX: u64 = PA_V | PA_R | PA_W | PA_X | PA_G;
pub const PA_KERNEL_RW: u64 = PA_V | PA_R | PA_W | PA_G;
pub const PA_KERNEL_RO: u64 = PA_V | PA_R | PA_G;

pub const LEVEL0_PAGE_SIZE: usize = 4096; // 4KB page
pub const LEVEL1_PAGE_SIZE: usize = 2 * 1024 * 1024; // 2MB huge page
pub const LEVEL2_PAGE_SIZE: usize = 1 * 1024 * 1024 * 1024; // 1GB huge page

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct PTE64(pub u64);

#[derive(Clone, Copy)]
pub struct PageAttribute64(u64);

pub struct RawPageTableSv48<'a>(NonNull<PTE64>, PhantomData<&'a ()>);

pub struct PagingModeSv48;

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
}

impl PagingMode for PagingModeSv48 {
    type Entry = PTE64;
    type RawTable<'a> = RawPageTableSv48<'a>;
    const LEVELS: &'static [PageTableLevel] = &[
        PageTableLevel::new(39, 9),
        PageTableLevel::new(30, 9),
        PageTableLevel::new(21, 9),
        PageTableLevel::new(12, 9),
    ];
    const KERNEL_ROOT_TABLE_PFN: PFN = PAGE_TABLE_BASE;
}

impl<'a> RawPageTable<'a> for RawPageTableSv48<'a> {
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

        if self.0 & (PA_R | PA_W | PA_X) != 0 {
            panic!("Encountered a huge page while parsing table attributes");
        }

        if self.0 & PA_V != 0 {
            table_attr |= TableAttribute::PRESENT;
        }
        if self.0 & PA_G != 0 {
            table_attr |= TableAttribute::GLOBAL;
        }
        if self.0 & PA_U != 0 {
            table_attr |= TableAttribute::USER;
        }
        if self.0 & PA_A != 0 {
            table_attr |= TableAttribute::ACCESSED;
        }

        Some(table_attr)
    }

    fn as_page_attr(self) -> Option<PageAttribute> {
        let mut page_attr = PageAttribute::READ;

        if self.0 & PA_V != 0 {
            page_attr |= PageAttribute::PRESENT;
        }

        if self.0 & PA_R != 0 {
            page_attr |= PageAttribute::READ;
        }

        if self.0 & PA_W != 0 {
            page_attr |= PageAttribute::WRITE;
        }

        if self.0 & PA_X != 0 {
            page_attr |= PageAttribute::EXECUTE;
        }

        if self.0 & PA_U != 0 {
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

        /*if self.0 & PA_ANON != 0 {
            page_attr |= PageAttribute::ANONYMOUS;
        }*/

        Some(page_attr)
    }

    fn from_table_attr(table_attr: TableAttribute) -> Self {
        let mut raw_attr = PA_W | PA_R;

        for attr in table_attr.iter() {
            match attr {
                TableAttribute::PRESENT => raw_attr |= PA_V,
                TableAttribute::GLOBAL => raw_attr |= PA_G,
                TableAttribute::USER => raw_attr |= PA_U,
                TableAttribute::ACCESSED => raw_attr |= PA_A,
                _ => unreachable!("Invalid table attribute"),
            }
        }

        Self(raw_attr)
    }

    fn from_page_attr(page_attr: PageAttribute) -> Self {
        let mut raw_attr = 0;

        for attr in page_attr.iter() {
            match attr {
                PageAttribute::PRESENT => raw_attr |= PA_V,
                PageAttribute::READ => raw_attr |= PA_R,
                PageAttribute::WRITE => raw_attr |= PA_W,
                PageAttribute::EXECUTE => raw_attr |= PA_X,
                PageAttribute::USER => raw_attr |= PA_U,
                PageAttribute::ACCESSED => raw_attr |= PA_A,
                PageAttribute::DIRTY => raw_attr |= PA_D,
                PageAttribute::GLOBAL => raw_attr |= PA_G,
                PageAttribute::COPY_ON_WRITE => raw_attr |= PA_COW,
                PageAttribute::MAPPED => raw_attr |= PA_MMAP,
                PageAttribute::ANONYMOUS => {},
                _ => unreachable!("Invalid page attribute"),
            }
        }

        Self(raw_attr)
    }
}

pub type DefaultPagingMode = PagingModeSv48;
