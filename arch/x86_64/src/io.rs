use core::arch::asm;

pub fn enable_sse() {
    unsafe {
        asm!(
            "mov %cr0, %rax",
            "and $(~0xc), %rax",
            "or $0x22, %rax",
            "mov %rax, %cr0",
            "mov %cr4, %rax",
            "or $0x600, %rax",
            "mov %rax, %cr4",
            "fninit",
            out("rax") _,
            options(att_syntax, nomem, nostack)
        )
    }
}

pub fn inb(no: u16) -> u8 {
    let data;
    unsafe {
        asm!(
            "inb %dx, %al",
            in("dx") no,
            out("al") data,
            options(att_syntax, nomem, nostack)
        )
    };

    data
}

pub fn inw(no: u16) -> u16 {
    let data;
    unsafe {
        asm!(
            "inw %dx, %ax",
            in("dx") no,
            out("ax") data,
            options(att_syntax, nomem, nostack)
        )
    };

    data
}

pub fn inl(no: u16) -> u32 {
    let data;
    unsafe {
        asm!(
            "inl %dx, %eax",
            in("dx") no,
            out("eax") data,
            options(att_syntax, nomem, nostack)
        )
    };

    data
}

pub fn outb(no: u16, data: u8) {
    unsafe {
        asm!(
            "outb %al, %dx",
            in("al") data,
            in("dx") no,
            options(att_syntax, nomem, nostack)
        )
    };
}

pub fn outw(no: u16, data: u16) {
    unsafe {
        asm!(
            "outw %ax, %dx",
            in("ax") data,
            in("dx") no,
            options(att_syntax, nomem, nostack)
        )
    };
}

pub fn outl(no: u16, data: u32) {
    unsafe {
        asm!(
            "outl %eax, %dx",
            in("eax") data,
            in("dx") no,
            options(att_syntax, nomem, nostack)
        )
    };
}
