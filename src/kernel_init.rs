use crate::{
    kernel::{
        self,
        cpu::init_localcpu,
        mem::{AsMemoryBlock, GlobalPageAlloc, KernelPageAccess, RawPage},
    },
    kernel_init,
};
use arch::DefaultPagingMode;
use eonix_mm::{
    address::{Addr as _, AddrOps as _, PAddr, PRange, VAddr, VRange},
    page_table::{PageAttribute, PagingMode as _, PTE},
    paging::{NoAlloc, Page as GenericPage, PAGE_SIZE, PFN},
};
use eonix_runtime::context::ExecutionContext;
use eonix_sync::LazyLock;

static GLOBAL_PAGE_TABLE: LazyLock<
    eonix_mm::page_table::PageTable<DefaultPagingMode, NoAlloc, KernelPageAccess>,
> = LazyLock::new(|| unsafe {
    GenericPage::with_raw(
        DefaultPagingMode::KERNEL_ROOT_TABLE_PFN,
        |root_table_page| eonix_mm::page_table::PageTable::with_root_table(root_table_page.clone()),
    )
});

const HUGE_PAGE_LEN: usize = 1 << 21;

const P_KERNEL_BSS_START: PAddr = PAddr::from_val(0x200000);
const P_KIMAGE_START: PAddr = PAddr::from_val(0x400000);

const V_KERNEL_PAGE_ARRAY_START: VAddr = VAddr::from(0xffffff8040000000);
const V_KERNEL_BSS_START: VAddr = VAddr::from(0xffffffffc0200000);
const KERNEL_BSS_LEN: usize = HUGE_PAGE_LEN;

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
struct BootLoaderData {
    entry_count: u32,
    entry_length: u32,

    block_count_1k: u32,
    block_count_64k: u32,

    all_entries: [E820MemMapEntry; 42],
}

impl E820MemMapEntry {
    const ENTRY_FREE: u32 = 1;
    // const ENTRY_USED: u32 = 2;

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

#[no_mangle]
pub(self) extern "C" fn _kernel_init(bootloader_data: &mut BootLoaderData) -> ! {
    // Map kernel BSS
    for pte in GLOBAL_PAGE_TABLE.iter_kernel_levels(
        VRange::from(V_KERNEL_BSS_START).grow(KERNEL_BSS_LEN),
        &DefaultPagingMode::LEVELS[..3],
    ) {
        let attr = PageAttribute::PRESENT
            | PageAttribute::WRITE
            | PageAttribute::READ
            | PageAttribute::HUGE
            | PageAttribute::GLOBAL;

        pte.set(PFN::from(P_KERNEL_BSS_START), attr.into());
    }

    unsafe {
        // SAFETY: We've just mapped the area with sufficient length.
        core::ptr::write_bytes(V_KERNEL_BSS_START.addr() as *mut (), 0, KERNEL_BSS_LEN);
    }

    let addr_max = bootloader_data
        .free_entries()
        .map(|entry| entry.range().end())
        .max()
        .expect("No free memory");

    let pfn_max = PFN::from(addr_max.ceil());
    let len_bytes_page_array = usize::from(pfn_max) * size_of::<RawPage>();

    let count_huge_pages = len_bytes_page_array.div_ceil(HUGE_PAGE_LEN);

    extern "C" {
        // Definition inside linker script.
        fn KIMAGE_PAGES();
    }

    let kimage_pages = unsafe { core::mem::transmute::<_, usize>(KIMAGE_PAGES as *const ()) };

    let paddr_after_kimage = P_KIMAGE_START + kimage_pages * PAGE_SIZE;
    let paddr_after_kimage_aligned = paddr_after_kimage.ceil_to(HUGE_PAGE_LEN);

    let mut paddr_free = paddr_after_kimage_aligned;

    // Map kernel page array.
    for pte in GLOBAL_PAGE_TABLE.iter_kernel_levels(
        VRange::from(V_KERNEL_PAGE_ARRAY_START).grow(HUGE_PAGE_LEN * count_huge_pages),
        &DefaultPagingMode::LEVELS[..3],
    ) {
        let attr = PageAttribute::PRESENT
            | PageAttribute::WRITE
            | PageAttribute::READ
            | PageAttribute::HUGE
            | PageAttribute::GLOBAL;

        pte.set(PFN::from(paddr_free), attr.into());

        paddr_free = paddr_free + HUGE_PAGE_LEN;
    }

    unsafe {
        // SAFETY: We've just mapped the area with sufficient length.
        core::ptr::write_bytes(
            V_KERNEL_PAGE_ARRAY_START.addr() as *mut (),
            0,
            count_huge_pages * HUGE_PAGE_LEN,
        );
    }

    let paddr_unused_start = paddr_free;

    for entry in bootloader_data.free_entries() {
        let mut range = entry.range();

        GlobalPageAlloc::mark_present(range);

        if range.end() <= paddr_unused_start {
            continue;
        }

        if range.start() < paddr_unused_start {
            let (_, right) = range.split_at(paddr_unused_start);
            range = right;
        }

        unsafe {
            // SAFETY: We are in system initialization procedure where preemption is disabled.
            GlobalPageAlloc::add_pages(range);
        }
    }

    unsafe {
        // SAFETY: We are in system initialization procedure where preemption is disabled.
        GlobalPageAlloc::add_pages(PRange::new(PAddr::from(0x100000), PAddr::from(0x200000)));
        GlobalPageAlloc::add_pages(PRange::new(paddr_after_kimage, paddr_after_kimage_aligned));
    }

    let (stack_bottom_addr, stack_pfn) = {
        let kernel_stack_page = GenericPage::alloc_order_in(9, GlobalPageAlloc::early_alloc());
        let stack_area = kernel_stack_page.as_memblk();

        let stack_bottom_addr = stack_area
            .addr()
            .checked_add(stack_area.len())
            .expect("The stack bottom should not be null");

        let stack_pfn = kernel_stack_page.into_raw();

        (stack_bottom_addr, stack_pfn)
    };

    let mut to_ctx = ExecutionContext::new();
    to_ctx.set_interrupt(false);
    to_ctx.set_sp(stack_bottom_addr.get());
    to_ctx.call1(_init_on_new_stack, usize::from(stack_pfn));

    to_ctx.switch_noreturn();
}

extern "C" fn _init_on_new_stack(early_kernel_stack_pfn: PFN) -> ! {
    // Add the pages previously used by `_kernel_init` as a stack.
    unsafe {
        // SAFETY: We are in system initialization procedure where preemption is disabled.
        GlobalPageAlloc::add_pages(PRange::new(PAddr::from(0x8000), PAddr::from(0x80000)));
    }

    init_localcpu();

    extern "C" {
        fn init_allocator();
    }

    unsafe { init_allocator() };

    kernel::interrupt::init().unwrap();

    kernel_init(early_kernel_stack_pfn)
}
