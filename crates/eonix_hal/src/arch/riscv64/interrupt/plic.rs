use core::ptr::{read_volatile, write_volatile};

use super::super::config::platform::virt::*;

use riscv::register::mie;

pub struct PlicDriver {
    base_addr: usize,
    hart_id: usize,
}

impl PlicDriver {
    pub fn new(base_addr: usize, hart_id: usize) -> Self {
        let driver = PlicDriver { base_addr, hart_id };

        let s_context_id = driver.s_context_id();

        driver.set_priority_threshold(0);

        let enable_reg_base_for_context = driver.enable_addr(s_context_id);
        // PLIC enable bits are grouped into 32-bit registers.
        // Assuming 32 registers for up to 1024 IRQs, but 32 is common for a few hundred.
        for i in 0..(driver.max_irq_num() / 32 + 1) {
            let reg_addr = (enable_reg_base_for_context as usize + (i as usize) * 4) as *mut u32;
            unsafe {
                (reg_addr as *mut u32).write_volatile(0);
            }
        }

        unsafe {
            mie::set_sext();
        }

        driver.set_priority(10, 1);
        driver.enable_interrupt(10);

        // TODO: may need more set_priority
        // driver.set_priority(UART_IRQ_ID, 1);
        // driver.enable_interrupt(UART_IRQ_ID);
        // driver.set_priority(VIRTIO_BLOCK_IRQ_ID, 1);
        // driver.enable_interrupt(VIRTIO_BLOCK_IRQ_ID);

        driver
    }

    fn s_context_id(&self) -> usize {
        self.hart_id * PLIC_S_MODE_CONTEXT_STRIDE + 1
    }

    fn priority_addr(&self, irq_id: u32) -> *mut u32 {
        (self.base_addr + PLIC_PRIORITY_OFFSET + (irq_id as usize) * 4) as *mut u32
    }

    fn pending_addr(&self) -> *mut u32 {
        (self.base_addr + PLIC_PENDING_OFFSET) as *mut u32
    }

    fn enable_addr(&self, context_id: usize) -> *mut u32 {
        // PLIC enable bits are typically organized as 32-bit banks.
        // The offset for each context's enable registers.
        // A common stride for context enable blocks is 0x80 (128 bytes).
        (self.base_addr + PLIC_ENABLE_OFFSET + context_id * PLIC_ENABLE_PER_HART_OFFSET) as *mut u32
    }

    fn threshold_addr(&self, context_id: usize) -> *mut u32 {
        // A common stride for context threshold/claim/complete registers is 0x1000 (4KB).
        (self.base_addr + PLIC_THRESHOLD_OFFSET + context_id * PLIC_THRESHOLD_CLAIM_COMPLETE_PER_HART_OFFSET) as *mut u32
    }

    fn claim_complete_addr(&self, context_id: usize) -> *mut u32 {
        (self.base_addr + PLIC_CLAIM_COMPLETE_OFFSET + context_id * PLIC_THRESHOLD_CLAIM_COMPLETE_PER_HART_OFFSET) as *mut u32
    }

    pub fn set_priority(&self, irq_id: u32, priority: u32) {
        unsafe {
            write_volatile(self.priority_addr(irq_id), priority);
        }
    }

    pub fn get_priority(&self, irq_id: u32) -> u32 {
        unsafe { read_volatile(self.priority_addr(irq_id)) }
    }

    pub fn enable_interrupt(&self, irq_id: u32) {
        let context_id = self.s_context_id();
        let enable_reg_offset_in_bank = (irq_id / 32) as usize * 4;
        let bit_index_in_reg = irq_id % 32;

        let enable_reg_addr = (self.enable_addr(context_id) as usize + enable_reg_offset_in_bank) as *mut u32;
        let bit_mask = 1 << bit_index_in_reg;
        unsafe {
            let old = read_volatile(enable_reg_addr);
            write_volatile(enable_reg_addr, old | bit_mask);
        }
    }

    pub fn disable_interrupt(&self, irq_id: u32) {
        let context_id = self.s_context_id();
        let enable_reg_offset_in_bank = (irq_id / 32) as usize * 4;
        let bit_index_in_reg = irq_id % 32;

        let enable_reg_addr = (self.enable_addr(context_id) as usize + enable_reg_offset_in_bank) as *mut u32;
        let bit_mask = 1 << bit_index_in_reg;
        unsafe {
            let old = read_volatile(enable_reg_addr);
            write_volatile(enable_reg_addr, old & !bit_mask);
        }
    }

    pub fn set_priority_threshold(&self, threshold: u32) {
        let context_id = self.s_context_id();
        unsafe {
            write_volatile(self.threshold_addr(context_id), threshold);
        }
    }

    pub fn get_priority_threshold(&self) -> u32 {
        let context_id = self.s_context_id();
        unsafe { read_volatile(self.threshold_addr(context_id)) }
    }

    pub fn claim_interrupt(&self) -> u32 {
        let context_id = self.s_context_id();
        unsafe { read_volatile(self.claim_complete_addr(context_id)) }
    }

    pub fn complete_interrupt(&self, irq_id: u32) {
        let context_id = self.s_context_id();
        unsafe {
            write_volatile(self.claim_complete_addr(context_id), irq_id);
        }
    }

    pub fn max_irq_num(&self) -> u32 {
        127
    }
}
