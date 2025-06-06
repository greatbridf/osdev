mod page;

use page::setup_kernel_page_table;

use super::{
    config::{self, mm::*},
    fdt::get_num_harts,
};

use core::arch::naked_asm;

#[link_section = ".bss.stack"]
static mut BOOT_STACK: [u8; 4096 * 16] = [0; 4096 * 16];

#[repr(C, align(4096))]
struct BootPageTable([u64; PTES_PER_PAGE]);

/// map 0x8000 0000 to itself and 0xffff ffff 8000 0000
static mut BOOT_PAGE_TABLE: BootPageTable = {
    let mut arr: [u64; PTES_PER_PAGE] = [0; PTES_PER_PAGE];
    arr[2] = (0x80000 << 10) | 0xcf;
    arr[510] = (0x80000 << 10) | 0xcf;
    BootPageTable(arr)
};

extern "C" {
    fn kernel_init();
}

/// bootstrap in rust
#[naked]
#[no_mangle]
#[link_section = ".text.entry"]
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
            li   t2, {virt_ram_offset}
            or   sp, sp, t2
            la   t3, riscv64_start
            or   t3, t3, t2
            jalr t3                      // call riscv64_start
        ",
        boot_stack = sym BOOT_STACK,
        page_table = sym BOOT_PAGE_TABLE,
        virt_ram_offset = const KIMAGE_OFFSET,
    )
}

/// TODO: 
/// linker，现在VMA和LMA不对
/// kernel_init不知道要干什么
#[no_mangle]
pub unsafe extern "C" fn riscv64_start(hart_id: usize, dtb_addr: usize) -> ! {
    setup_kernel_page_table();
    let num_harts = get_num_harts(dtb_addr);
    config::smp::set_num_harts(num_harts);
    unsafe { kernel_init() };

    unreachable!();
}
