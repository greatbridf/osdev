use core::arch::asm;

#[derive(Clone, Copy)]
pub struct Port8 {
    no: u16,
}

impl Port8 {
    pub const fn new(no: u16) -> Self {
        Self { no }
    }

    pub fn read(&self) -> u8 {
        let data;
        unsafe {
            asm!(
                "inb %dx, %al",
                in("dx") self.no,
                out("al") data,
                options(att_syntax, nomem, nostack)
            )
        };

        data
    }

    pub fn write(&self, data: u8) {
        unsafe {
            asm!(
                "outb %al, %dx",
                in("al") data,
                in("dx") self.no,
                options(att_syntax, nomem, nostack)
            )
        };
    }
}
