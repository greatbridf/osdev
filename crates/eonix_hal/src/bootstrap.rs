use crate::mm::{BasicPageAlloc, BasicPageAllocRef};
use core::cell::RefCell;
use eonix_mm::address::PRange;

pub struct BootStrapData {
    pub(crate) early_stack: PRange,
    pub(crate) allocator: Option<RefCell<BasicPageAlloc>>,
}

impl BootStrapData {
    pub fn get_alloc(&self) -> Option<BasicPageAllocRef<'_>> {
        self.allocator.as_ref().map(BasicPageAllocRef::new)
    }

    pub fn take_alloc(&mut self) -> Option<BasicPageAlloc> {
        self.allocator.take().map(RefCell::into_inner)
    }

    pub fn get_early_stack(&self) -> PRange {
        self.early_stack
    }
}

#[cfg(target_arch = "riscv64")]
pub fn early_console_write(s: &str) {
    crate::arch::bootstrap::early_console_write(s);
}

#[cfg(target_arch = "riscv64")]
pub fn early_console_putchar(ch: u8) {
    crate::arch::bootstrap::early_console_putchar(ch);
}
