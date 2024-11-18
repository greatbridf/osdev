#![no_std]

pub mod vm {
    use core::arch::asm;

    #[inline(always)]
    pub fn invlpg(vaddr: usize) {
        unsafe {
            asm!(
                "invlpg ({})",
                in(reg) vaddr,
                options(att_syntax)
            );
        }
    }

    #[inline(always)]
    pub fn invlpg_all() {
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
    pub fn get_cr3() -> usize {
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
    pub fn set_cr3(pfn: usize) {
        unsafe {
            asm!(
                "mov %cr3, {0}",
                in(reg) pfn,
                options(att_syntax)
            );
        }
    }
}

pub mod interrupt;
pub mod io;
pub mod task;
