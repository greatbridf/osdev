mod context;
mod gdt;
mod init;
mod interrupt;
mod io;
mod mm;
mod percpu;
mod user;

pub use self::context::*;
pub use self::gdt::*;
pub use self::init::*;
pub use self::interrupt::*;
pub use self::io::*;
pub use self::user::*;
pub use mm::*;
pub use percpu::*;
pub use percpu_macros::{define_percpu, define_percpu_shared};

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

#[inline(always)]
pub fn rdmsr(msr: u32) -> u64 {
    let edx: u32;
    let eax: u32;

    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") eax,
            out("edx") edx,
            options(att_syntax),
        );
    }

    (edx as u64) << 32 | eax as u64
}

#[inline(always)]
pub fn wrmsr(msr: u32, value: u64) {
    let eax = value as u32;
    let edx = (value >> 32) as u32;

    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") eax,
            in("edx") edx,
            options(att_syntax),
        );
    }
}
