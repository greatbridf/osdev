use core::arch::asm;

use eonix_mm::{address::{Addr, PAddr, VAddr}, paging::PFN};
use riscv::{asm::{sfence_vma, sfence_vma_all}, register::{satp, stval}};

pub mod io;
pub mod fence;
pub mod fpu;

#[inline(always)]
pub fn flush_tlb(vaddr: usize) {
    sfence_vma(vaddr, 0);
}

#[inline(always)]
pub fn flush_tlb_all() {
    sfence_vma_all();
}

#[inline(always)]
pub fn get_root_page_table_pfn() -> PFN {
    let satp_val = satp::read();
    let ppn = satp_val.ppn();
    PFN::from(ppn)
}

#[inline(always)]
pub fn set_root_page_table_pfn(pfn: PFN) {
    let ppn = PAddr::from(pfn).addr();

    unsafe { satp::set(satp::Mode::Sv39, 0, ppn) };
    sfence_vma_all();
}

#[inline(always)]
pub fn get_page_fault_address() -> VAddr {
    VAddr::from(stval::read())
}

#[inline(always)]
pub fn halt() {
    unsafe {
        asm!(
            "wfi",
            options(nomem, nostack)
        );
    }
}

#[inline(always)]
pub fn pause() {
    unsafe {
        asm!(
            "nop",
            options(nomem, nostack)
        );
    }
}
