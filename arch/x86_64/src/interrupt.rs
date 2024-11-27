use core::arch::asm;

pub fn enable() {
    unsafe {
        asm!("sti");
    }
}

pub fn disable() {
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
