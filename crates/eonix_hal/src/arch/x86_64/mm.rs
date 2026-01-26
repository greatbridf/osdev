use core::arch::asm;
use core::marker::PhantomData;
use core::ptr::NonNull;

use eonix_mm::address::{
    Addr as _, AddrOps as _, PAddr, PRange, PhysAccess, VAddr,
};
use eonix_mm::page_table::{
    PageAttribute, PageTable, PageTableAlloc, PageTableLevel, PagingMode,
    RawAttribute, RawPageTable, TableAttribute, PTE,
};
use eonix_mm::paging::{BasicFolio, PageAccess, PageBlock, PAGE_SIZE, PFN};
use eonix_sync_base::LazyLock;

use crate::traits::mm::Memory;

pub const PA_P: u64 = 0x001;
pub const PA_RW: u64 = 0x002;
pub const PA_US: u64 = 0x004;
#[allow(dead_code)]
pub const PA_PWT: u64 = 0x008;
#[allow(dead_code)]
pub const PA_PCD: u64 = 0x010;
pub const PA_A: u64 = 0x020;
pub const PA_D: u64 = 0x040;
pub const PA_PS: u64 = 0x080;
pub const PA_G: u64 = 0x100;
pub const PA_COW: u64 = 0x200;
pub const PA_MMAP: u64 = 0x400;
pub const PA_ANON: u64 = 0x800;
pub const PA_NXE: u64 = 0x8000_0000_0000_0000;
pub const PA_MASK: u64 = 0xfff0_0000_0000_0fff;

pub const P_KIMAGE_START: PAddr = PAddr::from_val(0x200000);
pub const V_KERNEL_BSS_START: VAddr = VAddr::from(0xffffffffc0200000);

const KERNEL_PML4_PFN: PFN = PFN::from_val(0x1000 >> 12);
pub const GLOBAL_PAGE_TABLE: BasicFolio = BasicFolio::new(KERNEL_PML4_PFN, 0);

#[repr(transparent)]
pub struct PTE64(u64);

#[derive(Clone, Copy)]
pub struct PageAttribute64(u64);

pub struct RawPageTable4Levels<'a>(NonNull<PTE64>, PhantomData<&'a ()>);

pub struct PagingMode4Levels;

pub struct ArchPhysAccess;

#[derive(Clone)]
pub struct PageAccessImpl;

pub struct ArchMemory;

#[repr(C)]
#[derive(Copy, Clone)]
struct E820MemMapEntry {
    base: u64,
    len: u64,
    entry_type: u32,
    acpi_attrs: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct BootLoaderData {
    entry_count: u32,
    entry_length: u32,

    block_count_1k: u32,
    block_count_64k: u32,

    all_entries: [E820MemMapEntry; 42],
}

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

unsafe impl Send for RawPageTable4Levels<'_> {}
unsafe impl Sync for RawPageTable4Levels<'_> {}

impl RawAttribute for PageAttribute64 {
    fn null() -> Self {
        Self(0)
    }

    fn as_table_attr(self) -> Option<TableAttribute> {
        let mut table_attr = TableAttribute::empty();

        if self.0 & PA_PS != 0 {
            return None;
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

pub type ArchPagingMode = PagingMode4Levels;

impl ArchPhysAccess {
    const PHYS_OFFSET: usize = 0xffff_ff00_0000_0000;
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

impl E820MemMapEntry {
    const ENTRY_FREE: u32 = 1;
    // const ENTRY_USED: u32 = 2;

    const fn zeroed() -> Self {
        Self {
            base: 0,
            len: 0,
            entry_type: 0,
            acpi_attrs: 0,
        }
    }

    fn is_free(&self) -> bool {
        self.entry_type == Self::ENTRY_FREE
    }

    // fn is_used(&self) -> bool {
    //     self.entry_type == Self::ENTRY_USED
    // }

    fn range(&self) -> PRange {
        PRange::from(PAddr::from(self.base as usize)).grow(self.len as usize)
    }
}

impl BootLoaderData {
    const fn zeroed() -> Self {
        Self {
            entry_count: 0,
            entry_length: 0,
            block_count_1k: 0,
            block_count_64k: 0,
            all_entries: [E820MemMapEntry::zeroed(); 42],
        }
    }

    // fn memory_size(&self) -> usize {
    //     // The initial 1M is not counted in the E820 map. We add them to the total as well.
    //     ((self.block_count_1k + 64 * self.block_count_64k) * 1024 + 1 * 1024 * 1024) as usize
    // }

    fn entries(&self) -> &[E820MemMapEntry] {
        &self.all_entries[..self.entry_count as usize]
    }

    fn free_entries(&self) -> impl Iterator<Item = &E820MemMapEntry> {
        self.entries().iter().filter(|entry| entry.is_free())
    }
}

#[unsafe(link_section = ".low")]
pub static mut E820_MEM_MAP_DATA: BootLoaderData = BootLoaderData::zeroed();

impl Memory for ArchMemory {
    fn present_ram() -> impl Iterator<Item = PRange> {
        let e820 = &raw const E820_MEM_MAP_DATA;

        unsafe {
            // SAFETY: We don't write to the E820 memory map after the bootstrap.
            e820.as_ref()
                .unwrap_unchecked()
                .free_entries()
                .map(|entry| entry.range())
        }
    }

    fn free_ram() -> impl Iterator<Item = PRange> {
        unsafe extern "C" {
            fn KIMAGE_PAGES();
        }

        let kimage_pages = KIMAGE_PAGES as usize;

        let paddr_after_kimage = P_KIMAGE_START + kimage_pages * PAGE_SIZE;
        let paddr_after_kimage_aligned = paddr_after_kimage.ceil_to(0x200000);
        let paddr_unused_start = paddr_after_kimage_aligned;

        core::iter::once(PRange::new(
            PAddr::from_val(0x100000),
            PAddr::from_val(0x200000),
        ))
        .chain(core::iter::once(PRange::new(
            paddr_after_kimage,
            paddr_after_kimage_aligned,
        )))
        .chain(
            Self::present_ram()
                .filter(move |range| range.end() > paddr_unused_start)
                .map(move |range| {
                    if range.start() < paddr_unused_start {
                        let (_, right) = range.split_at(paddr_unused_start);
                        right
                    } else {
                        range
                    }
                }),
        )
    }
}

#[inline(always)]
pub fn flush_tlb(vaddr: usize) {
    unsafe {
        asm!(
            "invlpg ({})",
            in(reg) vaddr,
            options(att_syntax)
        );
    }
}

#[inline(always)]
pub fn flush_tlb_all() {
    unsafe {
        asm!(
            "mov %cr3, %rax",
            "mov %rax, %cr3",
            out("rax") _,
            options(att_syntax)
        );
    }
}

#[inline(always)]
pub fn get_root_page_table_pfn() -> PFN {
    let cr3: usize;
    unsafe {
        asm!(
            "mov %cr3, {0}",
            out(reg) cr3,
            options(att_syntax)
        );
    }
    PFN::from(PAddr::from(cr3))
}

#[inline(always)]
pub fn set_root_page_table_pfn(pfn: PFN) {
    unsafe {
        asm!(
            "mov {0}, %cr3",
            in(reg) PAddr::from(pfn).addr(),
            options(att_syntax)
        );
    }
}

pub fn with_global_page_table<A, X>(
    alloc: A, access: X,
    func: impl FnOnce(&mut PageTable<ArchPagingMode, A, X>),
) where
    A: PageTableAlloc<Folio = BasicFolio>,
    X: PageAccess,
{
    let mut global_page_table =
        PageTable::new(GLOBAL_PAGE_TABLE.clone(), alloc, access);

    func(&mut global_page_table);

    core::mem::forget(global_page_table);
}
