use super::super::config::platform::virt::*;
use core::ptr;

use sbi::{
    ipi::send_ipi,
    timer::set_timer,
    HartMask,
    SbiError
};
use riscv::register::mie;

/// CLINT (Core Local Interruptor) driver
/// This struct now owns the base address and hart_id.
pub struct ClintDriver {
    base_addr: usize,
    hart_id: usize,
}

impl ClintDriver {
    pub fn new(base_addr: usize, hart_id: usize) -> Self {
        let driver = ClintDriver { base_addr, hart_id };

        driver.clear_soft_interrupt_pending(hart_id);

        // Enable Supervisor-mode Software Interrupts (SSIE)
        // and Supervisor-mode Timer Interrupts (STIE) in the `mie` CSR.
        unsafe {
            mie::set_ssoft(); // Enable S-mode Software Interrupts
            mie::set_stimer(); // Enable S-mode Timer Interrupts
        }

        driver
    }

    /// Reads the current value of the global MTIME (Machine Timer) counter.
    pub fn get_time(&self) -> u64 {
        unsafe {
            // MTIME is a 64-bit counter at CLINT_BASE + CLINT_MTIME_OFFSET
            ptr::read_volatile((self.base_addr + CLINT_MTIME_OFFSET) as *mut u64)
        }
    }

    /// Sets the next timer interrupt trigger point using SBI.
    pub fn set_timer(&self, time_value: u64) -> Result<(), SbiError> {
        set_timer(time_value)
    }

    /// Sends an Inter-Processor Interrupt (IPI) to the specified Hart(s).
    pub fn send_ipi(&self, hart_id_mask: usize) -> Result<(), SbiError> {
        // This utilizes the SBI `send_ipi` call.
        send_ipi(HartMask::from(hart_id_mask))
    }

    /// Clears the software interrupt pending bit for the specified Hart.
    pub fn clear_soft_interrupt_pending(&self, hart_id: usize) {
        unsafe {
            // MSIP registers are typically located at CLINT_BASE + 4 * hart_id.
            // Writing 0 to the register clears the pending bit.
            ptr::write_volatile((self.base_addr + CLINT_MSIP_OFFSET + hart_id * 4) as *mut u32, 0);
        }
    }
}
