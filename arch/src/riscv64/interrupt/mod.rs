mod plic;
mod clint;

pub use plic::*;
pub use clint::*;

/// TODO:
/// 一开始的中断汇编

use riscv::register::sstatus::{self, Sstatus};
use sbi::SbiError;

use super::platform::virt::*;

/// Floating-point registers context.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct FpuRegisters {
    pub f: [u64; 32],
    pub fcsr: u32,
}

/// Saved CPU context when a trap (interrupt or exception) occurs on RISC-V 64.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct TrapContext {
    pub x: [usize; 32],

    // CSRs
    pub sstatus: usize, // sstatus CSR value. Contains privilege mode, interrupt enable, FPU state.
    pub sepc: usize,    // sepc (Supervisor Exception Program Counter). Program counter at trap.
    pub scause: usize,  // scause (Supervisor Cause). Describes the cause of the trap.
    pub stval: usize,   // stval (Supervisor Trap Value). Contains faulting address for exceptions.
    pub satp: usize,    // satp (Supervisor Address Translation and Protection). Page table base.

    // may need to save
    // pub sscratch: usize, // sscratch (Supervisor Scratch).

    // FPU
    // pub fpu_regs: FpuRegisters,
}

impl TrapContext {
    pub fn set_return_value(&mut self, value: usize) {
        // a0, x10
        self.x[10] = value;
    }

    pub fn set_return_address(&mut self, addr: usize, user: bool) {
        self.sepc = addr; // 设置 Supervisor Exception Program Counter

        // if user==true,set SPP to U-mode (0)
        // if user==false, set SPP to S-mode (1)
        if user {
            self.sstatus &= !(1 << 8); // clear SPP bit
        } else {
            self.sstatus |= 1 << 8;  // set SPP bit
        }
    }

    pub fn set_stack_pointer(&mut self, sp: usize, _user: bool) {
        self.x[2] = sp;
    }

    pub fn set_interrupt_enabled(&mut self, enabled: bool) {
        // S mode Previous Interrupt Enable (SPIE)
        if enabled {
            self.sstatus |= 1 << 5;
        } else {
            self.sstatus &= !(1 << 5);
        }
    }
}

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
