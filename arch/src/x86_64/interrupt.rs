use core::arch::asm;

/// Saved registers when a trap (interrupt or exception) occurs.
#[allow(missing_docs)]
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct InterruptContext {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rbp: u64,
    
    pub int_no: u64,
    pub error_code: u64,

    // Pushed by CPU
    pub rip: u64,
    pub cs: u64,
    pub eflags: u64,
    pub rsp: u64,
    pub ss: u64,
}


pub fn enable_irqs() {
    unsafe {
        asm!("sti");
    }
}

pub fn disable_irqs() {
    unsafe {
        asm!("cli");
    }
}

pub fn lidt(base: usize, limit: u16) {
    let mut idt_descriptor = [0u16; 5];

    idt_descriptor[0] = limit;
    idt_descriptor[1] = base as u16;
    idt_descriptor[2] = (base >> 16) as u16;
    idt_descriptor[3] = (base >> 32) as u16;
    idt_descriptor[4] = (base >> 48) as u16;

    unsafe {
        asm!("lidt ({})", in(reg) &idt_descriptor, options(att_syntax));
    }
}
