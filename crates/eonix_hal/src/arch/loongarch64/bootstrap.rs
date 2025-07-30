use super::cpu::CPUID;
use super::cpu::CPU_COUNT;
use crate::{
    arch::{
        cpu::CPU,
        mm::{ArchPhysAccess, PageAttribute64, GLOBAL_PAGE_TABLE},
        trap::CSR_KERNEL_TP,
    },
    bootstrap::BootStrapData,
    mm::{
        flush_tlb_all, ArchMemory, ArchPagingMode, BasicPageAlloc, BasicPageAllocRef,
        ScopedAllocator,
    },
};
use core::arch::naked_asm;
use core::{
    alloc::Allocator,
    arch::asm,
    cell::RefCell,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use eonix_hal_traits::mm::Memory;
use eonix_mm::{
    address::{Addr as _, PAddr, PRange, PhysAccess, VAddr, VRange},
    page_table::{PageAttribute, PagingMode, PTE as _},
    paging::{Page, PageAccess, PageAlloc, PAGE_SIZE, PFN},
};
use eonix_percpu::PercpuArea;
use loongArch64::register::ecfg;
use loongArch64::register::ecfg::LineBasedInterrupt;
use loongArch64::register::tcfg;
use loongArch64::register::{euen, pgdl};

#[unsafe(link_section = ".bootstrap.stack")]
static BOOT_STACK: [u8; 4096 * 16] = [0; 4096 * 16];
static BOOT_STACK_START: &'static [u8; 4096 * 16] = &BOOT_STACK;

#[repr(C, align(4096))]
struct PageTable([u64; 512]);

/// map 0x8000_0000 to 0x8000_0000 and 0xffff_ffff_8000_0000
#[unsafe(link_section = ".bootstrap.page_table.1")]
static BOOT_PAGE_TABLE: PageTable = {
    let mut arr = [0; 512];
    arr[0] = 0 | 0x11d3; // G | W | P | H | Cached | D | V
    arr[510] = 0 | 0x11d3; // G | W | P | H | Cached | D | V
    arr[511] = 0x8000_2000 | (1 << 60); // PT1, PT

    PageTable(arr)
};

#[unsafe(link_section = ".bootstrap.page_table.2")]
#[used]
static PT1: PageTable = {
    let mut arr = [0; 512];
    arr[510] = 0x8000_0000 | 0x11d3; // G | W | P | H | Cached | D | V

    PageTable(arr)
};

/// bootstrap in rust
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".bootstrap.entry")]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        "
            li.d      $t0, 0xc
            csrwr     $t0, {CSR_STLB_PAGE_SIZE}

            li.d      $t0, {PWCL}
            csrwr     $t0, {CSR_PWCL}

            li.d      $t0, {PWCH}
            csrwr     $t0, {CSR_PWCH}

            la.global $t0, {tlb_refill_entry}
            csrwr     $t0, {CSR_TLB_REFILL_ENTRY}

            la.global $t0, {page_table}
            move      $t1, $t0
            csrwr     $t0, {CSR_PGDL}
            csrwr     $t1, {CSR_PGDH}

            dbar      0x0
            invtlb    0x0, $zero, $zero

            csrrd     $t0, {CSR_CRMD}
            li.d      $t1, ~0x18
            and       $t0, $t0, $t1
            ori       $t0, $t0,  0x10
            csrwr     $t0, {CSR_CRMD}

            la.global $sp, {boot_stack}
            li.d      $t0, 0xffffff0000000000
            or        $sp, $sp, $t0
            li.d      $t0, {BOOT_STACK_SIZE}
            add.d     $sp, $sp, $t0

            csrrd     $a0, {CSR_CPUID}
            move      $ra, $zero

            la.global $t0, {riscv64_start}
            jirl      $zero, $t0, 0
        ",
        boot_stack = sym BOOT_STACK,
        BOOT_STACK_SIZE = const size_of_val(&BOOT_STACK),
        CSR_CRMD = const 0x00,
        CSR_PGDL = const 0x19,
        CSR_PGDH = const 0x1a,
        CSR_PWCL = const 0x1c,
        CSR_PWCH = const 0x1d,
        CSR_STLB_PAGE_SIZE = const 0x1e,
        CSR_CPUID = const 0x20,
        CSR_TLB_REFILL_ENTRY = const 0x88,
        PWCL = const (12 << 0) | (9 << 5) | (21 << 10) | (9 << 15) | (30 << 20) | (9 << 25) | (0 << 30),
        PWCH = const (39 << 0) | (9 << 6),
        tlb_refill_entry = sym tlb_refill_entry,
        page_table = sym BOOT_PAGE_TABLE,
        riscv64_start = sym riscv64_start,
    )
}

#[unsafe(naked)]
#[unsafe(link_section = ".bootstrap.tlb_fill_entry")]
unsafe extern "C" fn tlb_refill_entry() {
    naked_asm!(
        "csrwr   $t0, {CSR_TLBRSAVE}",
        "csrrd   $t0, {CSR_PGD}",
        "lddir   $t0, $t0, 3",
        "lddir   $t0, $t0, 2",
        "lddir   $t0, $t0, 1",
        "ldpte   $t0, 0",
        "ldpte   $t0, 1",
        "tlbfill",
        "csrrd   $t0, {CSR_TLBRSAVE}",
        "ertn",
        CSR_TLBRSAVE = const 0x8b,
        CSR_PGD = const 0x1b,
    )
}

/// TODO:
/// 启动所有的cpu
pub unsafe extern "C" fn riscv64_start(hart_id: usize) -> ! {
    pgdl::set_base(0xffff_ffff_ffff_0000);
    flush_tlb_all();

    let real_allocator = RefCell::new(BasicPageAlloc::new());
    let alloc = BasicPageAllocRef::new(&real_allocator);

    for range in ArchMemory::free_ram() {
        real_allocator.borrow_mut().add_range(range);
    }

    setup_kernel_page_table(&alloc);

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
        early_stack: PRange::new(
            PAddr::from(start),
            PAddr::from(start + size_of_val(&BOOT_STACK)),
        ),
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

fn setup_kernel_page_table(alloc: impl PageAlloc) {
    let global_page_table = &GLOBAL_PAGE_TABLE;

    let attr = PageAttribute::WRITE
        | PageAttribute::READ
        | PageAttribute::EXECUTE
        | PageAttribute::GLOBAL
        | PageAttribute::PRESENT
        | PageAttribute::ACCESSED
        | PageAttribute::DIRTY;

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

    flush_tlb_all();

    unsafe {
        core::ptr::write_bytes(KERNEL_BSS_START.addr() as *mut (), 0, BSS_LENGTH as usize);
    }
}

/// set up tp register to percpu
fn setup_cpu(alloc: impl PageAlloc, hart_id: usize) {
    // enable FPU
    euen::set_fpe(true);
    euen::set_sxe(true);

    CPU_COUNT.fetch_add(1, Ordering::Relaxed);

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
                "move $tp, {0}",
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
        asm!(
            "csrwr {tp}, {CSR_KERNEL_TP}",
            tp = inout(reg) PercpuArea::get_for(cpu.cpuid()).unwrap().as_ptr() => _,
            CSR_KERNEL_TP = const CSR_KERNEL_TP,
        )
    }

    let timer_frequency = loongArch64::time::get_timer_freq();

    // 1ms periodic timer.
    tcfg::set_init_val(timer_frequency / 1_000);
    tcfg::set_periodic(true);
    tcfg::set_en(true);

    ecfg::set_lie(LineBasedInterrupt::all());
}

/// TODO
fn bootstrap_smp(alloc: impl Allocator, page_alloc: &RefCell<BasicPageAlloc>) {}

pub fn shutdown() -> ! {
    let ged_addr = PAddr::from(0x100E001C);
    unsafe {
        let ged_ptr = ArchPhysAccess::as_ptr::<u8>(ged_addr);
        ged_ptr.write_volatile(0x34);
        loop {}
    }
}
