use super::{
    config::{self, mm::*},
    mm::*,
    fdt::get_num_harts,
};

use core::{
    arch::global_asm, ptr::NonNull, sync::atomic::AtomicUsize
};
use intrusive_list::{container_of, Link};
use buddy_allocator::{BuddyAllocator, BuddyRawPage};
use riscv::{asm::sfence_vma_all, register::satp};
use eonix_mm::{
    address::{Addr as _, PAddr, VAddr, VRange},
    page_table::{PageAttribute, PagingMode, RawAttribute, PTE as _},
    paging::{Page, PageAccess, PageAlloc, PageBlock, RawPage as RawPageTrait, PFN},
};
use spin::Mutex;

global_asm!("start.S");

static mut PAGES: [RawPage; 1024] = [const { RawPage::new() }; 1024];

fn page(index: usize) -> &'static mut RawPage {
    let page = unsafe { PAGES.as_mut_ptr().add(index) };
    unsafe { &mut *page }
}

#[derive(Clone, Copy)]
struct RawPageHandle(usize);

impl From<PFN> for RawPageHandle {
    fn from(pfn: PFN) -> Self {
        assert!(usize::from(pfn) - ROOT_PAGE_TABLE_PFN < 1024, "PFN out of range");

        Self(usize::from(pfn) - ROOT_PAGE_TABLE_PFN)
    }
}

impl From<RawPageHandle> for PFN {
    fn from(raw_page: RawPageHandle) -> Self {
        PFN::from(raw_page.0 + ROOT_PAGE_TABLE_PFN)
    }
}

impl RawPageTrait for RawPageHandle {
    fn order(&self) -> u32 {
        page(self.0).order
    }

    fn refcount(&self) -> &AtomicUsize {
        &page(self.0).refcount
    }

    fn is_present(&self) -> bool {
        self.0 < 1024
    }
}

impl BuddyRawPage for RawPageHandle {
    unsafe fn from_link(link: &mut Link) -> Self {
        let page = container_of!(link, RawPage, link);
        let page_index = page.as_ptr().offset_from_unsigned(PAGES.as_ptr());
        assert!(page_index < 1024, "Page index out of range");

        Self(page_index)
    }

    unsafe fn get_link(&self) -> &mut Link {
        &mut page(self.0).link
    }

    fn set_order(&self, order: u32) {
        page(self.0).order = order;
    }

    fn is_buddy(&self) -> bool {
        page(self.0).buddy
    }

    fn is_free(&self) -> bool {
        page(self.0).free
    }

    fn set_buddy(&self) {
        page(self.0).buddy = true;
    }

    fn set_free(&self) {
        page(self.0).free = true;
    }

    fn clear_buddy(&self) {
        page(self.0).buddy = false;
    }

    fn clear_free(&self) {
        page(self.0).free = false;
    }
}

struct RawPage {
    link: Link,
    free: bool,
    buddy: bool,
    order: u32,
    refcount: AtomicUsize,
}

impl RawPage {
    const fn new() -> Self {
        Self {
            link: Link::new(),
            free: false,
            buddy: false,
            order: 0,
            refcount: AtomicUsize::new(0),
        }
    }
}

struct DirectPageAccess;

impl PageAccess for DirectPageAccess {
    unsafe fn get_ptr_for_pfn(pfn: PFN) -> NonNull<PageBlock> {
        unsafe { NonNull::new_unchecked(PAddr::from(pfn).addr() as *mut _) }
    }
}

static BUDDY: Mutex<BuddyAllocator<RawPageHandle>> = Mutex::new(BuddyAllocator::new());

#[derive(Clone)]
struct BuddyPageAlloc;

impl PageAlloc for BuddyPageAlloc {
    type RawPage = RawPageHandle;

    fn alloc_order(&self, order: u32) -> Option<Self::RawPage> {
        let retval = BUDDY.lock().alloc_order(order);
        retval
    }

    unsafe fn dealloc(&self, raw_page: Self::RawPage) {
        BUDDY.lock().dealloc(raw_page);
    }

    fn has_management_over(&self, page_ptr: Self::RawPage) -> bool {
        BuddyAllocator::has_management_over(page_ptr)
    }
}

type PageTable<'a> = eonix_mm::page_table::PageTable<'a, PagingModeSv48, BuddyPageAlloc, DirectPageAccess>;

fn setup_kernel_page_table() {
    let attr = PageAttribute::WRITE
        | PageAttribute::READ
        | PageAttribute::EXECUTE
        | PageAttribute::GLOBAL
        | PageAttribute::PRESENT;

    BUDDY.lock().create_pages(PAddr::from(ROOT_PAGE_TABLE_PHYS_ADDR), PAddr::from(PAGE_TABLE_PHYS_END));

    let root_table_page = Page::alloc_in(BuddyPageAlloc);
    let page_table = PageTable::new_in(&root_table_page, BuddyPageAlloc);

    // Map 0x80200000-0x81200000 16MB identically, use 2MB page
    for (idx, pte) in page_table
        .iter_kernel_levels(VRange::from(VAddr::from(KIMAGE_PHYS_BASE)).grow(0x1000000), &PagingModeSv48::LEVELS[..=2])
        .enumerate()
    {
        pte.set(PFN::from(idx * 0x200 + 0x80200), PageAttribute64::from_page_attr(attr));
    }

    // Map 0x0000_0000_0000_0000-0x0000_007F_FFFF_FFFF 512GB
    // to 0xFFFF_FF00_0000_0000 to 0xFFFF_FF7F_FFFF_FFFF, use 1 GB page
    for (idx, pte) in page_table
        .iter_kernel_levels(VRange::from(VAddr::from(PHYS_MAP_VIRT)).grow(0x80_0000_0000), &PagingModeSv48::LEVELS[..=1])
        .enumerate()
    {
        pte.set(PFN::from(idx * 0x40000), PageAttribute64::from_page_attr(attr));
    }

    // Map 2 MB kernel image
    for (idx, pte) in page_table
        .iter_kernel(VRange::from(VAddr::from(KIMAGE_VIRT_BASE)).grow(0x20_0000))
        .enumerate()
    {
        pte.set(PFN::from(idx + 0x80200), PageAttribute64::from_page_attr(attr));
    }


    unsafe {
        satp::set(satp::Mode::Sv48, 0, PFN::from(page_table.addr()).into());
    }
    sfence_vma_all();
}

extern "C" {
    fn kernel_init();
}

/// TODO: 
/// linker，现在VMA和LMA不对
/// 现在的地址空间可能要改一改，改回Sv39的，Sv48有点大了
#[no_mangle]
pub unsafe extern "C" fn riscv64_start(hart_id: usize, dtb_addr: usize) -> ! {
    let num_harts = get_num_harts(dtb_addr);
    config::smp::set_num_harts(num_harts);
    setup_kernel_page_table();
    unsafe { kernel_init() };

    unreachable!();
}
