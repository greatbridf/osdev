use core::{marker::PhantomData, ptr::NonNull};
use eonix_mm::{
    page_table::{
        PageAttribute, PageTableLevel, PagingMode, RawAttribute, RawPageTable, TableAttribute, PTE,
    },
    paging::{PageBlock, PFN},
};
use riscv::{asm::sfence_vma_all, register::satp};

use super::config::mm::ROOT_PAGE_TABLE_PFN;

pub const PAGE_TABLE_BASE: PFN = PFN::from_val(ROOT_PAGE_TABLE_PFN);

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
// Bit 0-9 (V, R, W, X, U, G, A, D, RSW)
#[allow(dead_code)]
pub const PA_FLAGS_MASK: u64 = 0x3FF; // 0b11_1111_1111

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct PTE64(pub u64);

#[derive(Clone, Copy)]
pub struct PageAttribute64(u64);

pub struct RawPageTableSv39<'a>(NonNull<PTE64>, PhantomData<&'a ()>);

pub struct PagingModeSv39;

impl PTE for PTE64 {
    type Attr = PageAttribute64;

    fn set(&mut self, pfn: PFN, attr: Self::Attr) {
        self.0 = (usize::from(pfn) << PA_SHIFT) as u64 | attr.0;
    }

    fn get(&self) -> (PFN, Self::Attr) {
        let pfn = PFN::from(self.0 as usize >> PA_SHIFT);
        let attr = PageAttribute64(self.0 & PA_FLAGS_MASK);
        (pfn, attr)
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
        unsafe { self.0.add(index as usize).as_ref() }
    }

    fn index_mut(&mut self, index: u16) -> &'a mut Self::Entry {
        unsafe { self.0.add(index as usize).as_mut() }
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
        let mut page_attr = PageAttribute::empty();

        if self.0 & (PA_R | PA_W | PA_X) == 0 {
            panic!("Invalid page attribute combination");
        }

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
        let mut raw_attr = 0;

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

pub type DefaultPagingMode = PagingModeSv39;

pub fn setup_kernel_satp() {
    unsafe {
        satp::set(satp::Mode::Sv48, 0, PFN::from(ROOT_PAGE_TABLE_PFN).into());
    }
    sfence_vma_all();
}
