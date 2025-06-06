use super::super::{
    config::mm::*,
    mm::*,
};

use core::{
    ptr::NonNull,
    sync::atomic::AtomicUsize
};
use intrusive_list::{container_of, Link};
use buddy_allocator::{BuddyAllocator, BuddyRawPage};
use riscv::{asm::sfence_vma_all, register::satp};
use eonix_mm::{
    address::{Addr as _, AddrOps, PAddr, VAddr, VRange},
    page_table::{PageAttribute, PagingMode, RawAttribute, PTE as _},
    paging::{Page, PageAccess, PageAlloc, PageBlock, RawPage as RawPageTrait, PFN},
};
use spin::Mutex;

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

type PageTable<'a> = eonix_mm::page_table::PageTable<'a, PagingModeSv39, BuddyPageAlloc, DirectPageAccess>;

extern "C" {
    fn _ekernel();
}

/// TODO:
/// _ekernel现在还没有，需要在linker里加上
/// 对kernel image添加更细的控制，或者不加也行
fn map_area(page_table: &PageTable, attr: PageAttribute, range: VRange, phy_offest: usize, page_size: PageSize) {
    let (pfn_size, levels) = match page_size {
        PageSize::_4KbPage => (0x1, &PagingModeSv39::LEVELS[..=0]),
        PageSize::_2MbPage => (0x200, &PagingModeSv39::LEVELS[..=1]),
        PageSize::_1GbPage => (0x40000, &PagingModeSv39::LEVELS[..=2]),
    };
    for (idx, pte) in page_table
        .iter_kernel_levels(range, levels)
        .enumerate()
    {
        pte.set(PFN::from(idx * pfn_size + phy_offest), PageAttribute64::from_page_attr(attr));
    }
}

/// Map physical memory after ekernel, about 0x8040 0000-0x20_7fff_fff about 128GB
/// to add a 0xffff ffc0 0000 0000 offest
/// first use 4KB page, then 2MB page, last 1GB page
fn map_free_physical_memory(attr: PageAttribute, page_table: &PageTable) {
    let ekernel = _ekernel as usize - KIMAGE_OFFSET;

    let start = PAddr::from(ekernel).ceil_to(PageSize::_4KbPage as usize);
    let end = PAddr::from(ekernel).ceil_to(PageSize::_2MbPage as usize);
    let size_4kb = end - start;
    let range = VRange::from(VAddr::from(PHYS_MAP_VIRT + start.addr())).grow(size_4kb);
    let pfn_start = start.addr() >> PAGE_SIZE_BITS;
    map_area(page_table, attr, range, pfn_start, PageSize::_4KbPage);

    let start = end;
    let end = start.ceil_to(PageSize::_1GbPage as usize);
    let size_2mb = end - start;
    let range = VRange::from(VAddr::from(PHYS_MAP_VIRT + start.addr())).grow(size_2mb);
    let pfn_start = start.addr() >> PAGE_SIZE_BITS;
    map_area(page_table, attr, range, pfn_start, PageSize::_2MbPage);

    let start = end;
    let size_1gb = MEMORY_SIZE;
    let range = VRange::from(VAddr::from(PHYS_MAP_VIRT + start.addr())).grow(size_1gb);
    let pfn_start = start.addr() >> PAGE_SIZE_BITS;
    map_area(page_table, attr, range, pfn_start, PageSize::_1GbPage);
}

pub fn setup_kernel_page_table() {
    let attr = PageAttribute::WRITE
        | PageAttribute::READ
        | PageAttribute::EXECUTE
        | PageAttribute::GLOBAL
        | PageAttribute::PRESENT;

    BUDDY.lock().create_pages(PAddr::from(ROOT_PAGE_TABLE_PHYS_ADDR), PAddr::from(PAGE_TABLE_PHYS_END));

    let root_table_page = Page::alloc_in(BuddyPageAlloc);
    let page_table = PageTable::new_in(&root_table_page, BuddyPageAlloc);

    // Map 0x00000000-0x7fffffff 2GB MMIO,
    // to 0xffff ffff 0000 0000 to 0xffff ffff 7ffff ffff, use 1GB page
    map_area(&page_table,
        attr,
        VRange::from(VAddr::from(MMIO_VIRT_BASE)).grow(0x2000_0000),
        0,
        PageSize::_1GbPage);
    
    map_free_physical_memory(attr, &page_table);

    // Map 2 MB kernel image
    for (idx, pte) in page_table
        .iter_kernel(VRange::from(VAddr::from(KIMAGE_VIRT_BASE)).grow(0x20_0000))
        .enumerate()
    {
        pte.set(PFN::from(idx + 0x80200), PageAttribute64::from_page_attr(attr));
    }

    unsafe {
        satp::set(
            satp::Mode::Sv39,
            0,
            usize::from(PFN::from(page_table.addr())),
        );
    }
    sfence_vma_all();
}
