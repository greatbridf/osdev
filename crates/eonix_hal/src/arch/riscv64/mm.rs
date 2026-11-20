use core::marker::PhantomData;
use core::ptr::NonNull;

use eonix_hal_traits::mm::Memory;
use eonix_mm::address::{Addr as _, AddrOps, PAddr, PRange, PhysAccess, VAddr};
use eonix_mm::arch::RawPTE;
use eonix_mm::page_table::{
    PageTable, PageTableLevel, PagingMode, RawPageTable,
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

pub struct RawPageTableSv48<'a>(NonNull<RawPTE>, PhantomData<&'a ()>);

pub struct PagingModeSv48;

pub struct ArchPhysAccess;

pub struct ArchMemory;

#[derive(Clone)]
pub struct PageAccessImpl;

impl PagingMode for PagingModeSv48 {
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
    fn index(&self, index: u16) -> &'a RawPTE {
        unsafe { self.0.add(index as usize).as_ref() }
    }

    fn index_mut(&mut self, index: u16) -> &'a mut RawPTE {
        unsafe { self.0.add(index as usize).as_mut() }
    }

    unsafe fn from_ptr(ptr: NonNull<PageBlock>) -> Self {
        Self(ptr.cast(), PhantomData)
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
