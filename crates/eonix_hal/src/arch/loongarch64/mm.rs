use crate::traits::mm::Memory;
use core::{
    arch::asm,
    marker::PhantomData,
    ptr::NonNull,
    sync::atomic::{compiler_fence, Ordering},
};
use eonix_mm::{
    address::{Addr as _, AddrOps, PAddr, PRange, PhysAccess, VAddr},
    page_table::{
        PageAttribute, PageTable, PageTableLevel, PagingMode, RawAttribute, RawPageTable,
        TableAttribute, PTE,
    },
    paging::{NoAlloc, Page, PageBlock, PAGE_SIZE, PFN},
};
use eonix_sync_base::LazyLock;
use loongArch64::register::pgdl;

pub const KIMAGE_OFFSET: usize = 0xffff_ffff_0000_0000;
pub const ROOT_PAGE_TABLE_PFN: usize = 0x8000_1000 >> 12;
pub const PAGE_TABLE_BASE: PFN = PFN::from_val(ROOT_PAGE_TABLE_PFN);
pub static GLOBAL_PAGE_TABLE: LazyLock<PageTable<ArchPagingMode, NoAlloc, ArchPhysAccess>> =
    LazyLock::new(|| unsafe {
        Page::with_raw(PAGE_TABLE_BASE, |root_table_page| {
            PageTable::with_root_table(root_table_page.clone())
        })
    });

pub const PA_VP: u64 = ((1 << 0) | (1 << 7));
pub const PA_D: u64 = 1 << 1;
pub const PA_U: u64 = 3 << 2;
pub const PA_CACHED: u64 = 1 << 4;
pub const PA_G: u64 = 1 << 6;
pub const PA_W: u64 = 1 << 8;
pub const PA_NR: u64 = 1 << 61;
pub const PA_NX: u64 = 1 << 62;

// in RSW
pub const PA_COW: u64 = 1 << 9;
pub const PA_MMAP: u64 = 1 << 10;

pub const PA_PT_USER: u64 = 1 << 59;
pub const PA_PT: u64 = 1 << 60;

pub const PA_FLAGS_MASK: u64 = 0xF800_0000_0000_0FFF;

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct PTE64(u64);

#[derive(Clone, Copy)]
pub struct PageAttribute64(u64);

pub struct RawPageTable48<'a>(NonNull<PTE64>, PhantomData<&'a ()>);

pub struct PagingMode48;

pub struct ArchPhysAccess;

pub struct ArchMemory;

impl PTE for PTE64 {
    type Attr = PageAttribute64;

    fn set(&mut self, pfn: PFN, attr: Self::Attr) {
        let pfn = ((usize::from(pfn) as u64) << 12) & !PA_FLAGS_MASK;
        self.0 = pfn | attr.0;
    }

    fn get(&self) -> (PFN, Self::Attr) {
        let pfn = PFN::from((self.0 & !PA_FLAGS_MASK) as usize >> 12);
        let attr = PageAttribute64(self.0 & PA_FLAGS_MASK);
        (pfn, attr)
    }
}

impl PagingMode for PagingMode48 {
    type Entry = PTE64;
    type RawTable<'a> = RawPageTable48<'a>;
    const LEVELS: &'static [PageTableLevel] = &[
        PageTableLevel::new(39, 9),
        PageTableLevel::new(30, 9),
        PageTableLevel::new(21, 9),
        PageTableLevel::new(12, 9),
    ];
}

pub type ArchPagingMode = PagingMode48;

impl<'a> RawPageTable<'a> for RawPageTable48<'a> {
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

        if self.0 & PA_PT == PA_PT {
            table_attr |= TableAttribute::PRESENT;
        }

        if self.0 & PA_PT_USER == PA_PT_USER {
            table_attr |= TableAttribute::USER;
        }

        Some(table_attr)
    }

    fn as_page_attr(self) -> Option<PageAttribute> {
        let mut page_attr = PageAttribute::empty();

        if self.0 & PA_PT == PA_PT {
            return None;
        }

        if self.0 & PA_VP == PA_VP {
            page_attr |= PageAttribute::PRESENT;
        }

        if self.0 & PA_NR == 0 {
            page_attr |= PageAttribute::READ;
        }

        if self.0 & PA_W != 0 {
            page_attr |= PageAttribute::WRITE;
        }

        if self.0 & PA_NX == 0 {
            page_attr |= PageAttribute::EXECUTE;
        }

        if self.0 & PA_U == PA_U {
            page_attr |= PageAttribute::USER;
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

        Some(page_attr)
    }
}

impl From<PageAttribute> for PageAttribute64 {
    fn from(page_attr: PageAttribute) -> Self {
        let mut raw_attr = PA_NR | PA_NX | PA_CACHED;

        for attr in page_attr.iter() {
            match attr {
                PageAttribute::PRESENT => raw_attr |= PA_VP,
                PageAttribute::READ => raw_attr &= !PA_NR,
                PageAttribute::WRITE => raw_attr |= PA_W,
                PageAttribute::EXECUTE => raw_attr &= !PA_NX,
                PageAttribute::USER => raw_attr |= PA_U,
                PageAttribute::DIRTY => raw_attr |= PA_D,
                PageAttribute::GLOBAL => raw_attr |= PA_G,
                PageAttribute::COPY_ON_WRITE => raw_attr |= PA_COW,
                PageAttribute::MAPPED => raw_attr |= PA_MMAP,
                PageAttribute::ACCESSED | PageAttribute::ANONYMOUS => {}
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
                TableAttribute::PRESENT => raw_attr |= PA_PT,
                TableAttribute::USER => raw_attr |= PA_PT_USER,
                TableAttribute::GLOBAL | TableAttribute::ACCESSED => {}
                _ => unreachable!("Invalid table attribute"),
            }
        }

        Self(raw_attr)
    }
}

impl ArchPhysAccess {
    const PHYS_OFFSET: usize = 0xffff_0000_0000_0000;
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
        let range1 = core::iter::once(PRange::from(PAddr::from_val(0)).grow(0x1000_0000));
        let range2 = core::iter::once(PRange::from(PAddr::from_val(0x8000_0000)).grow(0x3000_0000));

        range2.chain(range1)
    }

    fn free_ram() -> impl Iterator<Item = PRange> {
        unsafe extern "C" {
            fn __kernel_start();
            fn __kernel_end();
        }

        let kernel_start = PAddr::from(__kernel_start as usize - KIMAGE_OFFSET);
        let kernel_end = PAddr::from(__kernel_end as usize - KIMAGE_OFFSET);
        let paddr_after_kimage_aligned = kernel_end.ceil_to(PAGE_SIZE);

        Self::present_ram()
            .filter(move |range| {
                range.end() <= kernel_start || range.end() > paddr_after_kimage_aligned
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
    PFN::from(pgdl::read().base())
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
        asm!("dbar 0x0", "invtlb 0x3, $zero, $zero");
    }
}
