use super::{
    config::{self, mm::*},
    console::write_str,
    cpu::CPUID,
    time::set_next_timer,
    trap::TRAP_SCRATCH,
};
use crate::{
    arch::{
        cpu::CPU,
        fdt::{init_dtb_and_fdt, FdtExt},
        mm::{ArchPhysAccess, FreeRam, PageAttribute64, GLOBAL_PAGE_TABLE},
    },
    bootstrap::BootStrapData,
    mm::{ArchMemory, ArchPagingMode, BasicPageAlloc, BasicPageAllocRef, ScopedAllocator},
};
use core::arch::naked_asm;
use core::{
    alloc::Allocator,
    arch::asm,
    cell::RefCell,
    sync::atomic::{AtomicBool, AtomicUsize},
};
use eonix_hal_traits::mm::Memory;
use eonix_mm::{
    address::{Addr as _, PAddr, PRange, PhysAccess, VAddr, VRange},
    page_table::{PageAttribute, PagingMode, PTE as _},
    paging::{Page, PageAccess, PageAlloc, PAGE_SIZE, PFN},
};
use eonix_percpu::PercpuArea;
use fdt::Fdt;
use riscv::{asm::sfence_vma_all, register::satp};
use sbi::legacy::console_putchar;

#[unsafe(link_section = ".bootstrap.stack")]
static BOOT_STACK: [u8; 4096 * 16] = [0; 4096 * 16];

static BOOT_STACK_START: &'static [u8; 4096 * 16] = &BOOT_STACK;

#[repr(C, align(4096))]
struct PageTable([u64; PTES_PER_PAGE]);

/// map 0x8000 0000 to itself and 0xffff ffff 8000 0000
#[unsafe(link_section = ".bootstrap.page_table.1")]
static BOOT_PAGE_TABLE: PageTable = {
    let mut arr: [u64; PTES_PER_PAGE] = [0; PTES_PER_PAGE];
    arr[0] = 0 | 0x2f;
    arr[510] = 0 | 0x2f;
    arr[511] = (0x80202 << 10) | 0x21;

    PageTable(arr)
};

#[unsafe(link_section = ".bootstrap.page_table.2")]
#[used]
static PT1: PageTable = {
    let mut arr: [u64; PTES_PER_PAGE] = [0; PTES_PER_PAGE];
    arr[510] = (0x80000 << 10) | 0x2f;

    PageTable(arr)
};

/// bootstrap in rust
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".bootstrap.entry")]
unsafe extern "C" fn _start(hart_id: usize, dtb_addr: usize) -> ! {
    naked_asm!(
        "
            ld    sp, 2f
            li    t0, 0x10000
            add   sp, sp, t0
            ld    t0, 3f
            srli  t0, t0, 12
            li    t1, 9 << 60
            or    t0, t0, t1
            csrw  satp, t0
            sfence.vma
            ld    t0, 4f
            jalr  t0                      // call riscv64_start

            .pushsection .bootstrap.data, \"aw\", @progbits
            2:
            .8byte {boot_stack}
            3:
            .8byte {page_table}
            4:
            .8byte {riscv64_start}
            .popsection
        ",
        boot_stack = sym BOOT_STACK,
        page_table = sym BOOT_PAGE_TABLE,
        riscv64_start = sym riscv64_start,
    )
}

/// TODO:
/// 启动所有的cpu
pub unsafe extern "C" fn riscv64_start(hart_id: usize, dtb_addr: PAddr) -> ! {
    let fdt = Fdt::from_ptr(ArchPhysAccess::as_ptr(dtb_addr).as_ptr())
        .expect("Failed to parse DTB from static memory.");

    let real_allocator = RefCell::new(BasicPageAlloc::new());
    let alloc = BasicPageAllocRef::new(&real_allocator);

    for range in fdt.present_ram().free_ram() {
        real_allocator.borrow_mut().add_range(range);
    }

    setup_kernel_page_table(&alloc);
    unsafe {
        init_dtb_and_fdt(dtb_addr);
    }

    setup_cpu(&alloc, hart_id);

    // TODO: set up interrupt, smp
    ScopedAllocator::new(&mut [0; 1024])
        .with_alloc(|mem_alloc| bootstrap_smp(mem_alloc, &real_allocator));

    unsafe extern "Rust" {
        fn _eonix_hal_main(_: BootStrapData) -> !;
    }

    let start = unsafe {
        ((&BOOT_STACK_START) as *const &'static [u8; 4096 * 16]).read_volatile() as *const _
            as usize
    };
    let bootstrap_data = BootStrapData {
        early_stack: PRange::new(PAddr::from(start), PAddr::from(start + 4096 * 16)),
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
fn setup_kernel_page_table(alloc: impl PageAlloc) {
    let global_page_table = &GLOBAL_PAGE_TABLE;

    let attr = PageAttribute::WRITE
        | PageAttribute::READ
        | PageAttribute::EXECUTE
        | PageAttribute::GLOBAL
        | PageAttribute::PRESENT;

    const KERNEL_BSS_START: VAddr = VAddr::from(0xffffffff40000000);

    // Map kernel BSS
    for pte in global_page_table.iter_kernel_in(
        VRange::from(KERNEL_BSS_START).grow(BSS_LENGTH as usize),
        ArchPagingMode::LEVELS,
        &alloc,
    ) {
        let page = Page::alloc_in(&alloc);

        let attr = {
            let mut attr = attr.clone();
            attr.remove(PageAttribute::EXECUTE);
            attr
        };
        pte.set(page.into_raw(), attr.into());
    }

    sfence_vma_all();

    unsafe {
        core::ptr::write_bytes(KERNEL_BSS_START.addr() as *mut (), 0, BSS_LENGTH as usize);
    }

    unsafe {
        satp::set(
            satp::Mode::Sv48,
            0,
            usize::from(PFN::from(global_page_table.addr())),
        );
    }
    sfence_vma_all();
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

    CPUID.set(hart_id);

    let mut cpu = CPU::local();
    unsafe {
        cpu.as_mut().init();
    }

    percpu_area.register(cpu.cpuid());

    unsafe {
        // SAFETY: Interrupts are disabled.
        TRAP_SCRATCH
            .as_mut()
            .set_kernel_tp(PercpuArea::get_for(cpu.cpuid()).unwrap().cast());
    }

    // set current hart's mtimecmp register
    set_next_timer();
}

/// TODO
fn bootstrap_smp(alloc: impl Allocator, page_alloc: &RefCell<BasicPageAlloc>) {}

pub fn early_console_write(s: &str) {
    write_str(s);
}

pub fn early_console_putchar(ch: u8) {
    console_putchar(ch);
}
