mod context;
mod interrupt;
mod io;

pub use self::context::*;
pub use self::interrupt::*;
pub use self::io::*;

use core::arch::asm;

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
pub fn get_root_page_table() -> usize {
    let cr3: usize;
    unsafe {
        asm!(
            "mov %cr3, {0}",
            out(reg) cr3,
            options(att_syntax)
        );
    }
    cr3
}

#[inline(always)]
pub fn set_root_page_table(pfn: usize) {
    unsafe {
        asm!(
            "mov {0}, %cr3",
            in(reg) pfn,
            options(att_syntax)
        );
    }
}

#[inline(always)]
pub fn get_page_fault_address() -> usize {
    let cr2: usize;
    unsafe {
        asm!(
            "mov %cr2, {}",
            out(reg) cr2,
            options(att_syntax)
        );
    }
    cr2
}

#[inline(always)]
pub fn halt() {
    unsafe {
        asm!("hlt", options(att_syntax, nostack));
    }
}

#[inline(always)]
pub fn pause() {
    unsafe {
        asm!("pause", options(att_syntax, nostack));
    }
}

#[inline(always)]
pub fn freeze() -> ! {
    loop {
        interrupt::disable_irqs();
        halt();
    }
}
