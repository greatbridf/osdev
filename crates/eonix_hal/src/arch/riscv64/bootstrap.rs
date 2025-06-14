use crate::{
    arch::{
        cpu::CPU, fdt::init_dtb_and_fdt, mm::{ArchPhysAccess, PageAttribute64, PagingModeSv39, GLOBAL_PAGE_TABLE, V_KERNEL_BSS_START}
    },
    bootstrap::BootStrapData,
    mm::{ArchMemory, ArchPagingMode, BasicPageAlloc, BasicPageAllocRef, ScopedAllocator},
};
use riscv::{asm::sfence_vma_all, register::{satp, sstatus::{self, FS}}};
use core::{
    alloc::Allocator,
    arch::asm,
    cell::RefCell,
    sync::atomic::{AtomicBool, AtomicUsize},
};
use eonix_hal_traits::mm::Memory;
use eonix_mm::{
    address::{Addr as _, PAddr, PRange, VAddr, VRange},
    page_table::{PageAttribute, PagingMode, PTE as _},
    paging::{Page, PageAccess, PageAlloc, PAGE_SIZE, PFN},
};
use eonix_percpu::PercpuArea;

use super::{
    config::{self, mm::*},
    fdt::get_num_harts,
    console::write_str,
};

use core::arch::naked_asm;

#[unsafe(link_section = ".bootstack")]
static mut BOOT_STACK: [u8; 4096 * 16] = [0; 4096 * 16];

#[repr(C, align(4096))]
struct BootPageTable([u64; PTES_PER_PAGE]);

/// map 0x8000 0000 to itself and 0xffff ffff 8000 0000
#[unsafe(link_section = ".bootdata")]
static mut BOOT_PAGE_TABLE: BootPageTable = {
    let mut arr: [u64; PTES_PER_PAGE] = [0; PTES_PER_PAGE];
    arr[2] = (0x80000 << 10) | 0xcf;
    arr[510] = (0x80000 << 10) | 0xcf;
    BootPageTable(arr)
};

static AP_COUNT: AtomicUsize = AtomicUsize::new(0);
static AP_STACK: AtomicUsize = AtomicUsize::new(0);
static AP_SEM: AtomicBool = AtomicBool::new(false);

unsafe extern "Rust" {
    fn kernel_init();
}

/// bootstrap in rust
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
unsafe extern "C" fn _start(hart_id: usize, dtb_addr: usize) -> ! {
    naked_asm!(
        "
            la    sp, {boot_stack}
            la    t0, {page_table}
            srli  t0, t0, 12
            li    t1, 8 << 60
            or    t0, t0, t1
            csrw  satp, t0
            sfence.vma
            li    t2, {virt_ram_offset}
            or    sp, sp, t2
            la    t3, riscv64_start
            or    t3, t3, t2
            jalr  t3                      // call riscv64_start
        ",
        boot_stack = sym BOOT_STACK,
        page_table = sym BOOT_PAGE_TABLE,
        virt_ram_offset = const KIMAGE_OFFSET,
    )
}

/// TODO: 
/// linker，现在VMA和LMA不对
/// 设置中断
/// 启动所有的cpu
#[unsafe(no_mangle)]
pub unsafe extern "C" fn riscv64_start(hart_id: usize, dtb_addr: usize) -> ! {
    write_str("hello\n");

    let real_allocator = RefCell::new(BasicPageAlloc::new());
    let alloc = BasicPageAllocRef::new(&real_allocator);

    for range in ArchMemory::free_ram() {
        real_allocator.borrow_mut().add_range(range);
    }

    setup_kernel_page_table(&alloc);
    unsafe {
        init_dtb_and_fdt(dtb_addr)
    };
    enable_sse();
    
    let num_harts = get_num_harts();
    config::smp::set_num_harts(num_harts);

    setup_cpu(&alloc, hart_id);

    // TODO: set up interrupt, smp
    //ScopedAllocator::new(&mut [0; 1024])
    //    .with_alloc(|mem_alloc| bootstrap_smp(mem_alloc, &real_allocator));

    unsafe extern "Rust" {
        fn _eonix_hal_main(_: BootStrapData) -> !;
    }

    let start = &raw mut BOOT_STACK as usize + KIMAGE_OFFSET;
    let bootstrap_data = BootStrapData {
        early_stack: PRange::new(
            PAddr::from(start),
            PAddr::from(start + 4096 * 16)),
        allocator: Some(real_allocator),
    };

    unsafe {
        _eonix_hal_main(bootstrap_data);
    }
}

unsafe extern "C" {
    fn BSS_LENGTH();
    fn KIMAGE_PAGES();
}

/// TODO:
/// 对kernel image添加更细的控制，或者不加也行
pub fn setup_kernel_page_table(alloc: impl PageAlloc) {
    let global_page_table = &GLOBAL_PAGE_TABLE;

    let attr = PageAttribute::WRITE
        | PageAttribute::READ
        | PageAttribute::EXECUTE
        | PageAttribute::GLOBAL
        | PageAttribute::PRESENT;

    // Map Physical memory 128Gb, add a 0xFFFF_FFC0_0000_0000 offset
    // use 1 Gb size page
    for (idx, pte) in global_page_table
        .iter_kernel_levels(VRange::from(VAddr::from(PHYS_MAP_VIRT)).grow(MEMORY_SIZE), &PagingModeSv39::LEVELS[..=0])
        .enumerate()
    {
        pte.set(PFN::from(idx * 0x40000), PageAttribute64::from(attr));
    }

    // Map 2 MB kernel image
    for (idx, pte) in global_page_table
        .iter_kernel(VRange::from(VAddr::from(KIMAGE_VIRT_BASE)).grow(KIMAGE_PAGES as usize * 0x1000))
        .enumerate()
    {
        pte.set(PFN::from(idx + 0x80200), PageAttribute64::from(attr));
    }

    // Map kernel BSS
    for pte in global_page_table.iter_kernel_in(
        VRange::from(V_KERNEL_BSS_START).grow(BSS_LENGTH as usize),
        ArchPagingMode::LEVELS,
        &alloc,
    ) {
        let page = Page::alloc_in(&alloc);
        pte.set(page.into_raw(), attr.into());
    }

    unsafe {
        core::ptr::write_bytes(V_KERNEL_BSS_START.addr() as *mut (), 0, BSS_LENGTH as usize);
    }

    unsafe {
        satp::set(
            satp::Mode::Sv39,
            0,
            usize::from(PFN::from(global_page_table.addr())),
        );
    }
    sfence_vma_all();
}

pub fn enable_sse() {
    unsafe {
        // FS (Floating-point Status) Initial (0b01)
        sstatus::set_fs(FS::Initial);
    }
}

/// set up tp register to percpu
fn setup_cpu(alloc: impl PageAlloc, hart_id: usize) {
    let mut percpu_area = PercpuArea::new(|layout| {
        let page_count = layout.size().div_ceil(PAGE_SIZE);
        let page = Page::alloc_at_least_in(page_count, alloc);

        let ptr = ArchPhysAccess::get_ptr_for_page(&page).cast();
        page.into_raw();

        ptr
    });

    // set tp(x4) register
    percpu_area.setup(|pointer| {
        let percpu_base_addr = pointer.addr().get();
        unsafe {
            asm!(
                "mv tp, {0}",
                in(reg) percpu_base_addr,
                options(nostack, preserves_flags)
            );
        }
    });

    let mut cpu = CPU::local();
    unsafe {
        cpu.as_mut().init(hart_id);
    }
    
    percpu_area.register(cpu.cpuid());
}

/// TODO
fn bootstrap_smp(alloc: impl Allocator, page_alloc: &RefCell<BasicPageAlloc>) {

}
