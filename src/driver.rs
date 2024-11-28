pub mod ahci;
pub mod e1000e;
pub mod serial;

// TODO!!!: Put it somewhere else.
pub struct Port8 {
    no: u16,
}

impl Port8 {
    pub const fn new(no: u16) -> Self {
        Self { no }
    }

    pub fn read(&self) -> u8 {
        arch::inb(self.no)
    }

    pub fn write(&self, data: u8) {
        arch::outb(self.no, data)
    }
}
