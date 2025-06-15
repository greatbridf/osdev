pub mod ahci;
pub mod e1000e;
#[cfg(target_arch = "x86_64")]
pub mod serial;

#[cfg(target_arch = "x86_64")]
pub struct Port8 {
    no: u16,
}

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "riscv64")]
pub mod virtio;

#[cfg(target_arch = "riscv64")]
pub mod sbi_console;
