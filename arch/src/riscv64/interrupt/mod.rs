mod plic;
mod clint;
mod context;
mod trap;

use plic::*;
use clint::*;
use context::*;
use trap::*;

/// TODO:
/// 切换回到user的入口函数
/// percpu
/// user?

use riscv::{
    asm::sfence_vma_all,
    register::{
        sstatus::{self, Sstatus},
        stvec::{self, Stvec}
    }
};
use sbi::SbiError;
use core::arch::global_asm;

use super::platform::virt::*;

global_asm!(include_str!("trap.S"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqState(usize);

impl IrqState {
    #[inline]
    pub fn save() -> Self {
        let sstatus_val = sstatus::read().bits();

        unsafe {
            sstatus::clear_sie();
        }

        IrqState(sstatus_val)
    }

    #[inline]
    pub fn restore(self) {
        let Self(state) = self;
        unsafe {
            sstatus::write(Sstatus::from_bits(state));
        }
    }

    #[inline]
    pub fn was_enabled(&self) -> bool {
        (self.0 & (1 << 1)) != 0
    }
}

#[inline]
pub fn disable_interrupts() {
    unsafe {
        sstatus::clear_sie();
    }
}

#[inline]
pub fn enable_interrupts() {
    unsafe {
        sstatus::set_sie();
    }
}

/// Architecture-specific interrupt control block.
pub struct InterruptControl {
    hart_id: usize,
    plic: PlicDriver,
    clint: ClintDriver,
}

impl InterruptControl {
    /// # Safety
    /// should be called only once.
    pub(crate) fn new(hart_id: usize) -> Self {
        // Initialize PLICDriver for this Hart
        let plic = PlicDriver::new(PLIC_BASE, hart_id);

        // Initialize ClintDriver for this Hart
        let clint = ClintDriver::new(CLINT_BASE, hart_id);

        Self {
            hart_id,
            plic: plic,
            clint: clint,
        }
    }

    /// Configures the CLINT timer for a periodic interrupt.
    ///
    /// # Arguments
    /// * `interval_us`: The desired interval between timer interrupts in microseconds.
    pub fn setup_timer(&self, interval_us: u64) {
        let current_time = self.clint.get_time();
        // Calculate ticks per microsecond based on CPU_FREQ_HZ
        let ticks_per_us = CPU_FREQ_HZ / 1_000_000;
        let ticks = interval_us * ticks_per_us;

        let next_timer_at = current_time.checked_add(ticks).unwrap_or(u64::MAX);

        if let Err(e) = self.clint.set_timer(next_timer_at) {
            panic!("Failed to set CLINT timer: {:?}", e);
        }
    }

    /// Sends an Inter-Processor Interrupt (IPI) to target Harts.
    pub fn send_ipi(&self, target_hart_mask: usize) -> Result<(), SbiError> {
        self.clint.send_ipi(target_hart_mask)
    }

    /// Handles the "End Of Interrupt" (EOI) for the most recently claimed
    /// external interrupt.
    pub fn end_of_external_interrupt(&self, irq_id: u32) {
        self.plic.complete_interrupt(irq_id);
    }

    /// Clears the pending software interrupt for the current Hart.
    pub fn clear_soft_interrupt_pending(&self) {
        self.clint.clear_soft_interrupt_pending(self.hart_id);
    }

    pub fn plic_enable_interrupt(&self, irq_id: u32) {
        self.plic.enable_interrupt(irq_id);
    }

    pub fn plic_set_priority(&self, irq_id: u32, priority: u32) {
        self.plic.set_priority(irq_id, priority);
    }

    pub fn plic_claim_interrupt(&self) -> u32 {
        self.plic.claim_interrupt()
    }
}

extern "C" {
    fn trap_from_kernel();
    fn trap_from_user();
}

fn setup_trap_handler(trap_entry_addr: usize) {
    unsafe {
        stvec::write(Stvec::from_bits(trap_entry_addr));
    }
    sfence_vma_all();
}

pub fn setup_kernel_trap() {
    setup_trap_handler(trap_from_kernel as usize);
}

pub fn setup_user_trap() {
    setup_trap_handler(trap_from_user as usize);
}
