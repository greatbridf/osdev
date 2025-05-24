use core::{arch::{asm, naked_asm}, ptr};
use super::{mm::*, PAGE_SIZE};

pub const ROOT_PAGE_TABLE_PHYS_ADDR: usize = 0x8030_0000;
pub const KERNEL_PHYS_BASE: usize = 0x8000_0000;
pub const KIMAGE_VIRT_BASE: usize = 0xFFFF_FFFF_FFC0_0000;

#[link_section = ".bss.stack"]
static mut BOOT_STACK: [u8; 4096 * 16] = [0; 4096 * 16];

#[link_section = ".data.root_page_tables"]
static mut ROOT_PAGE_TABLES: [PTE64; 512] = [PTE64(0); 512];

#[link_section = ".data.boot_page_tables_lvl"]
#[used]
static mut LEVEL1_IDENTITY_TABLE: [PTE64; 512] = [PTE64(0); 512];

#[link_section = ".data.boot_page_tables_lvl"]
#[used]
static mut LEVEL1_KERNEL_TABLE: [PTE64; 512] = [PTE64(0); 512];

#[link_section = ".data.boot_page_tables_lvl"]
#[used]
static mut LEVEL0_KERNEL_TEXT_TABLE: [PTE64; 512] = [PTE64(0); 512];

#[link_section = ".data.boot_page_tables_lvl"]
#[used]
static mut LEVEL0_KERNEL_RODATA_TABLE: [PTE64; 512] = [PTE64(0); 512];

#[link_section = ".data.boot_page_tables_lvl"]
#[used]
static mut LEVEL0_KERNEL_DATA_TABLE: [PTE64; 512] = [PTE64(0); 512];

#[inline]
fn phys_to_ppn(phys_addr: usize) -> u64 {
    (phys_addr >> 12) as u64
}

#[inline]
fn virt_to_vpn_idx(virt_addr: usize, level: u8) -> usize {
    match level {
        2 => (virt_addr >> 30) & 0x1FF, // VPN[2] bits 38-30 (9 bits)
        1 => (virt_addr >> 21) & 0x1FF, // VPN[1] bits 29-21 (9 bits)
        0 => (virt_addr >> 12) & 0x1FF, // VPN[0] bits 20-12 (9 bits)
        _ => 0,
    }
}

fn fill_pte(page_table_entry: &mut PTE64, phys_addr: usize, flags: u64) {
    let ppn = phys_to_ppn(phys_addr);
    *page_table_entry = PTE64(ppn << 10 | flags); // PPN 10-53
}

fn setup_page_tables() {
    extern "C" {
        static TEXT_START: usize;
        static TEXT_END: usize;
        static RODATA_START: usize;
        static RODATA_END: usize;
        static DATA_START: usize;
        static DATA_END: usize;
    }
    unsafe {

        // 1. clear page table
        let root_page_tables_phys = ROOT_PAGE_TABLES.as_mut_ptr() as usize;
        let total_page_tables_size = (LEVEL0_KERNEL_DATA_TABLE.as_mut_ptr() as usize + PAGE_SIZE) - root_page_tables_phys;
        ptr::write_bytes(root_page_tables_phys as *mut u8, 0, total_page_tables_size);

        // 2. Identity Mapping
        // Level 2 (BOOT_PAGE_TABLES) -> Level 1 (LEVEL1_IDENTITY_TABLE) -> 2MB Huge Pages
        fill_pte(
            &mut ROOT_PAGE_TABLES[0],
            LEVEL1_IDENTITY_TABLE.as_ptr() as usize,
            PA_V
        );

        // LEVEL1_IDENTITY_TABLE (Level 1)
        // KERNEL_PHYS_BASE 1GB
        let identity_map_start_phys = KERNEL_PHYS_BASE;
        let identity_map_size = LEVEL2_PAGE_SIZE;

        let mut current_phys_addr = identity_map_start_phys;
        let end_phys_addr = identity_map_start_phys + identity_map_size;

        while current_phys_addr < end_phys_addr {
            let pte_idx_lvl1 = virt_to_vpn_idx(current_phys_addr, 1);
            fill_pte(
                &mut LEVEL1_IDENTITY_TABLE[pte_idx_lvl1],
                current_phys_addr, // 2MB
                PA_KERNEL_RWX
            );
            current_phys_addr += LEVEL1_PAGE_SIZE;
        }

        // 3. Kernel Space Mapping
        let kimage_vpn2_idx = virt_to_vpn_idx(KIMAGE_VIRT_BASE, 2);

        // ROOT_PAGE_TABLES (Level 2) -> LEVEL1_KERNEL_TABLE
        fill_pte(
            &mut ROOT_PAGE_TABLES[kimage_vpn2_idx],
            LEVEL1_KERNEL_TABLE.as_ptr() as usize,
            PA_V
        );

        let get_phys_addr = |virt_addr: usize, virt_base: usize, phys_base: usize| {
            phys_base + (virt_addr - virt_base)
        };

        // .text
        let text_virt_start = TEXT_START;
        let text_phys_start = get_phys_addr(TEXT_START, KIMAGE_VIRT_BASE, KERNEL_PHYS_BASE);
        let text_size = TEXT_END - TEXT_START;
        let text_vpn1_idx = virt_to_vpn_idx(text_virt_start, 1);
        
        fill_pte(
            &mut LEVEL1_KERNEL_TABLE[text_vpn1_idx],
            LEVEL0_KERNEL_TEXT_TABLE.as_ptr() as usize,
            PA_V
        );
        
        let mut current_virt = text_virt_start;
        let mut current_phys = text_phys_start;
        while current_virt < text_virt_start + text_size {
            let pte_idx_lvl0 = virt_to_vpn_idx(current_virt, 0);
            fill_pte(
                &mut LEVEL0_KERNEL_TEXT_TABLE[pte_idx_lvl0],
                current_phys,
                PA_KERNEL_RWX
            );
            current_virt += LEVEL0_PAGE_SIZE;
            current_phys += LEVEL0_PAGE_SIZE;
        }

        // .rodata
        let rodata_virt_start = RODATA_START;
        let rodata_phys_start = get_phys_addr(RODATA_START, KIMAGE_VIRT_BASE, KERNEL_PHYS_BASE);
        let rodata_size = RODATA_END - RODATA_START;
        let rodata_vpn1_idx = virt_to_vpn_idx(rodata_virt_start, 1);
        
        if rodata_vpn1_idx != text_vpn1_idx {
            fill_pte(
                &mut LEVEL1_KERNEL_TABLE[rodata_vpn1_idx],
                LEVEL0_KERNEL_RODATA_TABLE.as_ptr() as usize,
                PA_V
            );
        }
        
        current_virt = rodata_virt_start;
        current_phys = rodata_phys_start;
        while current_virt < rodata_virt_start + rodata_size {
            let pte_idx_lvl0 = virt_to_vpn_idx(current_virt, 0);
            fill_pte(
                &mut LEVEL0_KERNEL_RODATA_TABLE[pte_idx_lvl0],
                current_phys,
                PA_KERNEL_RO
            );
            current_virt += LEVEL0_PAGE_SIZE;
            current_phys += LEVEL0_PAGE_SIZE;
        }

        // .data 
        let data_virt_start = DATA_START;
        let data_phys_start = get_phys_addr(DATA_START, KIMAGE_VIRT_BASE, KERNEL_PHYS_BASE);
        let data_size = DATA_END - DATA_START;
        let data_vpn1_idx = virt_to_vpn_idx(data_virt_start, 1);

        if data_vpn1_idx != text_vpn1_idx && data_vpn1_idx != rodata_vpn1_idx {
            fill_pte(
                &mut LEVEL1_KERNEL_TABLE[data_vpn1_idx],
                LEVEL0_KERNEL_DATA_TABLE.as_ptr() as usize,
                PA_V
            );
        }
        
        current_virt = data_virt_start;
        current_phys = data_phys_start;
        while current_virt < data_virt_start + data_size {
            let pte_idx_lvl0 = virt_to_vpn_idx(current_virt, 0);
            fill_pte(
                &mut LEVEL0_KERNEL_DATA_TABLE[pte_idx_lvl0],
                current_phys,
                PA_KERNEL_RW
            );
            current_virt += LEVEL0_PAGE_SIZE;
            current_phys += LEVEL0_PAGE_SIZE;
        }
    }
}

fn enable_mmu() {
    unsafe {
        let satp_val = ROOT_PAGE_TABLE_PHYS_ADDR | (8 << 60); // Sv39 mode (8)
        
        asm!(
            "csrw satp, {satp_val}",
            "sfence.vma",
            satp_val = in(reg) satp_val,
        );
    }
}

extern "C" {
    fn kernel_init();
}

/// bootstrap in rust
#[naked]
#[no_mangle]
#[link_section = ".text.entry"]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        "la sp, {stack_top}",
        // TODO: set up page table, somewhere may be wrong
        "call {setup_page_tables_fn}",
        "call {enable_mmu_fn}",
        "jr {kernel_init_fn}",
        stack_top = sym BOOT_STACK,
        setup_page_tables_fn = sym setup_page_tables,
        enable_mmu_fn = sym enable_mmu,
        kernel_init_fn = sym kernel_init,
    )
}
