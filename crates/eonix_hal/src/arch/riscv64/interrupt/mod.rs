use core::pin::Pin;

use crate::arch::time;

use super::config::platform::virt::*;
use riscv::register::sie;
use riscv_peripheral::{
    aclint::{Clint, CLINT},
    plic::{Plic, PLIC},
};
use sbi::SbiError;

#[derive(Clone, Copy)]
struct ArchPlic;

#[derive(Clone, Copy)]
struct ArchClint;

unsafe impl Plic for ArchPlic {
    const BASE: usize = PLIC_BASE;
}

unsafe impl Clint for ArchClint {
    const BASE: usize = CLINT_BASE;
    const MTIME_FREQ: usize = CPU_FREQ_HZ as usize;
}

/// Architecture-specific interrupt control block.
pub struct InterruptControl {
    clint: CLINT<ArchClint>,
}

impl InterruptControl {
    /// # Safety
    /// should be called only once.
    pub(crate) fn new() -> Self {
        Self {
            clint: CLINT::new(),
        }
    }

    pub fn init(self: Pin<&mut Self>) {}
}

pub fn enable_timer_interrupt() {
    unsafe { sie::set_stimer() };
    time::set_next_timer();
}
