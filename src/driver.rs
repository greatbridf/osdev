pub mod ahci;
pub mod e1000e;
pub mod serial;
pub mod timer;

// TODO!!!: Put it somewhere else.
pub(self) struct Port8 {
    no: u16,
}

impl Port8 {
    const fn new(no: u16) -> Self {
        Self { no }
    }

    fn read(&self) -> u8 {
        arch::io::inb(self.no)
    }

    fn write(&self, data: u8) {
        arch::io::outb(self.no, data)
    }
}
