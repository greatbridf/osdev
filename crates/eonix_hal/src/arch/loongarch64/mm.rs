use core::arch::asm;
use core::marker::PhantomData;
use core::ptr::NonNull;
use core::sync::atomic::{compiler_fence, Ordering};

use eonix_mm::address::{Addr as _, AddrOps, PAddr, PRange, PhysAccess, VAddr};
use eonix_mm::arch::RawPTE;
use eonix_mm::page_table::{PTEntry, PageTableLevel, PagingMode, RawPageTable};
use eonix_mm::paging::{
    BasicFolio, Folio, PageAccess, PageBlock, PAGE_SIZE, PFN,
};
use loongArch64::register::pgdl;

use crate::traits::mm::Memory;

pub const KIMAGE_OFFSET: usize = 0xffff_ffff_0000_0000;
pub const ROOT_PAGE_TABLE_PFN: usize = 0x8000_1000 >> 12;
pub const PAGE_TABLE_BASE: PFN = PFN::from_val(ROOT_PAGE_TABLE_PFN);
pub const GLOBAL_PAGE_TABLE: BasicFolio = BasicFolio::new(PAGE_TABLE_BASE, 0);

pub struct RawPageTable48<'a>(NonNull<RawPTE>, PhantomData<&'a ()>);

pub struct PagingMode48;

pub struct ArchPhysAccess;

#[derive(Clone)]
pub struct PageAccessImpl;

pub struct ArchMemory;

impl PagingMode for PagingMode48 {
    type RawTable<'a> = RawPageTable48<'a>;
    const LEVELS: &'static [PageTableLevel] = &[
        PageTableLevel::new(39, 9),
        PageTableLevel::new(30, 9),
        PageTableLevel::new(21, 9),
        PageTableLevel::new(12, 9),
    ];
}

pub type ArchPagingMode = PagingMode48;

unsafe impl Send for RawPageTable48<'_> {}

impl<'a> RawPageTable<'a> for RawPageTable48<'a> {
    fn index(&self, index: u16) -> &'a PTEntry {
        unsafe { self.0.cast::<PTEntry>().add(index as usize).as_ref() }
    }

    unsafe fn from_ptr(ptr: NonNull<PageBlock>) -> Self {
        Self(ptr.cast(), PhantomData)
    }
}

impl ArchPhysAccess {
    const PHYS_OFFSET: usize = 0xffff_ff00_0000_0000;
}

impl PageAccess for PageAccessImpl {
    unsafe fn get_ptr_for_pfn(&self, pfn: PFN) -> NonNull<PageBlock> {
        unsafe { ArchPhysAccess::as_ptr(PAddr::from(pfn)) }
    }
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

impl Memory for ArchMemory {
    fn present_ram() -> impl Iterator<Item = PRange> {
        let range1 = core::iter::once(
            PRange::from(PAddr::from_val(0)).grow(0x1000_0000),
        );
        let range2 = core::iter::once(
            PRange::from(PAddr::from_val(0x8000_0000)).grow(0x3000_0000),
        );

        range2.chain(range1)
    }

    fn free_ram() -> impl Iterator<Item = PRange> {
        unsafe extern "C" {
            fn __kernel_start();
            fn __kernel_end_phys();
        }

        // Depending on the relocation, these symbols may appear either as
        // physical addresses or as kernel virtual addresses.
        let to_paddr = |addr: usize| {
            PAddr::from_val(if addr >= KIMAGE_OFFSET {
                addr - KIMAGE_OFFSET
            } else {
                addr
            })
        };

        let kernel_start = to_paddr(__kernel_start as usize);
        let kernel_end = to_paddr(__kernel_end_phys as usize);
        let paddr_after_kimage_aligned = kernel_end.ceil_to(PAGE_SIZE);

        Self::present_ram()
            .filter(move |range| {
                range.end() <= kernel_start
                    || range.end() > paddr_after_kimage_aligned
            })
            .map(move |range| {
                if range.end() > paddr_after_kimage_aligned
                    && range.start() < paddr_after_kimage_aligned
                {
                    let (_, right) = range.split_at(paddr_after_kimage_aligned);
                    right
                } else {
                    range
                }
            })
    }
}

pub type DefaultPagingMode = PagingMode48;

#[inline(always)]
pub fn flush_tlb(vaddr: usize) {
    unsafe {
        asm!(
            "dbar 0x0",
            "invtlb 0x5, $zero, {vaddr}",
            vaddr = in(reg) vaddr,
        );
    }
}

#[inline(always)]
pub fn flush_tlb_all() {
    unsafe {
        asm!("dbar 0x0", "invtlb 0x0, $zero, $zero");
    }
}

#[inline(always)]
pub fn get_root_page_table_pfn() -> PFN {
    PFN::from(PAddr::from(pgdl::read().base()))
}

#[inline(always)]
pub fn set_root_page_table_pfn(pfn: PFN) {
    compiler_fence(Ordering::SeqCst);

    unsafe {
        pgdl::set_base(PAddr::from(pfn).addr());
    }

    compiler_fence(Ordering::SeqCst);

    // Invalidate all user space TLB entries.
    unsafe {
        asm!("dbar 0x0", "invtlb 0x0, $zero, $zero");
    }
}
