extern crate alloc;


use crate::{arch::{fdt::get_global_fdt, riscv64::config::mm::KIMAGE_OFFSET}, traits::mm::Memory};
use core::{marker::PhantomData, ptr::NonNull};
use eonix_mm::{
    address::{Addr as _, AddrOps, PAddr, PRange, PhysAccess, VAddr}, page_table::{
        PageAttribute, PageTable, PageTableLevel, PagingMode, RawAttribute, RawPageTable, TableAttribute, PTE
    }, paging::{NoAlloc, Page, PageBlock, PFN}
};
use eonix_sync_base::LazyLock;
use fdt::Fdt;
use riscv::{asm::sfence_vma_all, register::satp};

use super::config::mm::{
    ROOT_PAGE_TABLE_PFN,
    PHYS_MAP_VIRT
};

pub const P_KIMAGE_START: PAddr = PAddr::from_val(0x80200000);
pub const V_KERNEL_BSS_START: VAddr = VAddr::from(0xffffffff00000000);

pub const PAGE_TABLE_BASE: PFN = PFN::from_val(ROOT_PAGE_TABLE_PFN);
pub static GLOBAL_PAGE_TABLE: LazyLock<PageTable<ArchPagingMode, NoAlloc, ArchPhysAccess>> =
    LazyLock::new(|| unsafe {
        Page::with_raw(PAGE_TABLE_BASE, |root_table_page| {
            PageTable::with_root_table(root_table_page.clone())
        })
    });

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

pub struct ArchPhysAccess;

pub struct ArchMemory;

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
}

pub type ArchPagingMode = PagingModeSv39;

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
                PageAttribute::ANONYMOUS => {},
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
                TableAttribute::USER => raw_attr |= PA_U,
                TableAttribute::ACCESSED => raw_attr |= PA_A,
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

impl Memory for ArchMemory {
    /// TODO: fuck, I don't know how to fix
    fn present_ram() -> impl Iterator<Item = eonix_mm::address::PRange> {
        let fdt: &'static Fdt<'static> = get_global_fdt();

        let memory_node = fdt.memory().clone();

        let owned_ranges: alloc::vec::Vec<PRange> = memory_node.regions() 
            .map(|mem_region| {
                let base = mem_region.starting_address as usize;
                let size = mem_region.size.expect("this region has no memory") as usize;
                PRange::from(PAddr::from(base as usize)).grow(size as usize)
            }).collect();
        
        owned_ranges.into_iter()
    }

    fn free_ram() -> impl Iterator<Item = eonix_mm::address::PRange> {
        unsafe extern "C" {
            fn __kernel_start();
            fn __kernel_end();
        }

        let kernel_end = PAddr::from(__kernel_end as usize - KIMAGE_OFFSET);
        let paddr_after_kimage_aligned = kernel_end.ceil_to(0x200000);

        core::iter::once(PRange::new(
            kernel_end,
            paddr_after_kimage_aligned,
        ))
        .chain(
            Self::present_ram()
                .filter(move |range| range.end() > paddr_after_kimage_aligned)
                .map(move |range| {
                    if range.start() < paddr_after_kimage_aligned {
                        let (_, right) = range.split_at(paddr_after_kimage_aligned);
                        right
                    } else {
                        range
                    }
                }),
        )
    }
}

pub type DefaultPagingMode = PagingModeSv39;

pub fn setup_kernel_satp() {
    unsafe {
        satp::set(satp::Mode::Sv39, 0, PFN::from(ROOT_PAGE_TABLE_PFN).into());
    }
    sfence_vma_all();
}
