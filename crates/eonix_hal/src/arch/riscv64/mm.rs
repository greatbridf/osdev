use core::marker::PhantomData;
use core::ptr::NonNull;

use eonix_hal_traits::mm::Memory;
use eonix_mm::address::{Addr as _, AddrOps, PAddr, PRange, PhysAccess, VAddr};
use eonix_mm::page_table::{
    PageAttribute, PageTable, PageTableLevel, PagingMode, RawAttribute,
    RawPageTable, TableAttribute, PTE,
};
use eonix_mm::paging::{BasicFolio, Folio, PageAccess, PageBlock, PFN};
use eonix_sync_base::LazyLock;
use fdt::Fdt;
use riscv::asm::{sfence_vma, sfence_vma_all};
use riscv::register::satp;

use super::config::mm::{PHYS_MAP_VIRT, ROOT_PAGE_TABLE_PFN};
use super::fdt::{FdtExt, FDT};
use crate::arch::riscv64::config::mm::KIMAGE_OFFSET;
use crate::extern_symbol_addr;
use crate::mm::BasicPageAlloc;

const PAGE_TABLE_BASE: PFN = PFN::from_val(ROOT_PAGE_TABLE_PFN);
pub const GLOBAL_PAGE_TABLE: BasicFolio = BasicFolio::new(PAGE_TABLE_BASE, 0);

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

pub struct RawPageTableSv48<'a>(NonNull<PTE64>, PhantomData<&'a ()>);

pub struct PagingModeSv48;

pub struct ArchPhysAccess;

pub struct ArchMemory;

#[derive(Clone)]
pub struct PageAccessImpl;

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

impl PagingMode for PagingModeSv48 {
    type Entry = PTE64;
    type RawTable<'a> = RawPageTableSv48<'a>;
    const LEVELS: &'static [PageTableLevel] = &[
        PageTableLevel::new(39, 9),
        PageTableLevel::new(30, 9),
        PageTableLevel::new(21, 9),
        PageTableLevel::new(12, 9),
    ];
}

pub type ArchPagingMode = PagingModeSv48;

unsafe impl Send for RawPageTableSv48<'_> {}

impl<'a> RawPageTable<'a> for RawPageTableSv48<'a> {
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

        if self.0 & PA_V != 0 {
            table_attr |= TableAttribute::PRESENT;
        }

        if table_attr.contains(TableAttribute::PRESENT)
            && self.0 & (PA_R | PA_W | PA_X) != 0
        {
            return None;
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

        if self.0 & PA_V != 0 {
            page_attr |= PageAttribute::PRESENT;
        }

        if page_attr.contains(PageAttribute::PRESENT)
            && (self.0 & (PA_R | PA_W | PA_X) == 0)
        {
            return None;
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
}

impl From<PageAttribute> for PageAttribute64 {
    fn from(page_attr: PageAttribute) -> Self {
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
                PageAttribute::ANONYMOUS => {}
                _ => unreachable!("Invalid page attribute"),
            }
        }

        Self(raw_attr)
    }
}

impl From<TableAttribute> for PageAttribute64 {
    fn from(table_attr: TableAttribute) -> Self {
        let mut raw_attr = 0;

        for attr in table_attr.iter() {
            match attr {
                TableAttribute::PRESENT => raw_attr |= PA_V,
                TableAttribute::GLOBAL => raw_attr |= PA_G,
                TableAttribute::USER | TableAttribute::ACCESSED => {}
                _ => unreachable!("Invalid table attribute"),
            }
        }

        Self(raw_attr)
    }
}

impl ArchPhysAccess {
    const PHYS_OFFSET: usize = PHYS_MAP_VIRT;
}

impl PhysAccess for ArchPhysAccess {
    unsafe fn as_ptr<T>(paddr: PAddr) -> NonNull<T> {
        let alignment: usize = align_of::<T>();
        assert!(paddr.addr() % alignment == 0, "Alignment error");

        unsafe {
            // SAFETY: We can assume that we'll never have `self.addr()` equals
            //         to `-PHYS_OFFSET`. Otherwise, the kernel might be broken.
            NonNull::new_unchecked((Self::PHYS_OFFSET + paddr.addr()) as *mut T)
        }
    }

    unsafe fn from_ptr<T>(ptr: NonNull<T>) -> PAddr {
        let addr = ptr.addr().get();

        assert!(addr % align_of::<T>() == 0, "Alignment error");
        assert!(
            addr >= Self::PHYS_OFFSET,
            "Address is not a valid physical address"
        );

        PAddr::from_val(addr - Self::PHYS_OFFSET)
    }
}

impl PageAccess for PageAccessImpl {
    unsafe fn get_ptr_for_pfn(&self, pfn: PFN) -> NonNull<PageBlock> {
        unsafe { ArchPhysAccess::as_ptr(PAddr::from(pfn)) }
    }
}

impl Memory for ArchMemory {
    fn present_ram() -> impl Iterator<Item = PRange> {
        FDT.present_ram()
    }

    fn free_ram() -> impl Iterator<Item = PRange> {
        FDT.free_ram()
    }
}

pub type DefaultPagingMode = PagingModeSv48;

#[inline(always)]
pub fn flush_tlb(vaddr: usize) {
    sfence_vma(0, vaddr);
}

#[inline(always)]
pub fn flush_tlb_all() {
    sfence_vma_all();
}

#[inline(always)]
pub fn get_root_page_table_pfn() -> PFN {
    let satp_val = satp::read();
    let ppn = satp_val.ppn();
    PFN::from(ppn)
}

#[inline(always)]
pub fn set_root_page_table_pfn(pfn: PFN) {
    unsafe { satp::set(satp::Mode::Sv48, 0, usize::from(pfn)) };
    sfence_vma_all();
}
