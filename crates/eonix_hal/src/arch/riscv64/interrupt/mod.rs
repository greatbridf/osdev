use super::{config::platform::virt::*, fence::memory_barrier, mm::ArchPhysAccess};
use crate::arch::time;
use crate::platform::PLIC_BASE;
use core::{pin::Pin, ptr::NonNull};
use eonix_mm::address::{PAddr, PhysAccess};
use eonix_sync_base::LazyLock;
use riscv::register::sie;
use sbi::SbiError;

const PRIORITY_OFFSET: usize = 0x0;
const PENDING_OFFSET: usize = 0x1000;

const ENABLE_OFFSET: usize = 0x2000;
const THRESHOLD_OFFSET: usize = 0x200000;
const CLAIM_COMPLETE_OFFSET: usize = 0x200004;

const ENABLE_STRIDE: usize = 0x80;
const CONTEXT_STRIDE: usize = 0x1000;

pub struct PLIC {
    enable: NonNull<u32>,
    threshold: NonNull<u32>,
    claim_complete: NonNull<u32>,
}

pub struct InterruptControl {
    pub plic: PLIC,
}

impl PLIC {
    fn new(cpuid: usize) -> Self {
        let base = PLIC_BASE;

        let enable = PAddr::from(base + (cpuid * 2 + 1) * ENABLE_STRIDE + ENABLE_OFFSET);
        let threshold = PAddr::from(base + (cpuid * 2 + 1) * CONTEXT_STRIDE + THRESHOLD_OFFSET);
        let claim_complete =
            PAddr::from(base + (cpuid * 2 + 1) * CONTEXT_STRIDE + CLAIM_COMPLETE_OFFSET);

        unsafe {
            // SAFETY: The PLIC registers are memory-mapped and placed at specific addresses.
            //         We are pretty sure that the addresses are valid.
            Self {
                enable: ArchPhysAccess::as_ptr(enable),
                threshold: ArchPhysAccess::as_ptr(threshold),
                claim_complete: ArchPhysAccess::as_ptr(claim_complete),
            }
        }
    }

    pub fn set_threshold(&self, threshold: u32) {
        unsafe {
            self.threshold.write_volatile(threshold);
        }
    }

    pub fn set_priority(&self, interrupt: usize, priority: u32) {
        let priority_ptr = unsafe {
            // SAFETY: The PLIC priority register is memory-mapped and placed at a specific address.
            //         We are pretty sure that the address is valid.
            ArchPhysAccess::as_ptr(PLIC_BASE + PRIORITY_OFFSET + interrupt * size_of::<u32>())
        };

        memory_barrier();

        unsafe {
            priority_ptr.write_volatile(priority);
        }

        memory_barrier();
    }

    pub fn claim_interrupt(&self) -> Option<usize> {
        match unsafe { self.claim_complete.read_volatile() } {
            0 => None,
            interrupt => Some(interrupt as usize),
        }
    }

    pub fn complete_interrupt(&self, interrupt: usize) {
        unsafe {
            self.claim_complete.write_volatile(interrupt as u32);
        }
    }

    pub fn enable_interrupt(&self, interrupt: usize) {
        debug_assert!(interrupt < 1024, "Interrupt number out of range");

        let enable_ptr = unsafe {
            // SAFETY: Interrupt number is guaranteed to be less than 1024,
            //         so we won't overflow the enable register array.
            self.enable.add(interrupt / 32)
        };

        let bit = 1 << (interrupt % 32);
        unsafe {
            enable_ptr.write_volatile(enable_ptr.read_volatile() | bit);
        }
    }

    pub fn disable_interrupt(&self, interrupt: usize) {
        let enable_ptr = unsafe {
            // SAFETY: Interrupt number is guaranteed to be less than 1024,
            //         so we won't overflow the enable register array.
            self.enable.add(interrupt / 32)
        };

        let bit = 1 << (interrupt % 32);
        unsafe {
            enable_ptr.write_volatile(enable_ptr.read_volatile() & !bit);
        }
    }
}

impl InterruptControl {
    /// # Safety
    /// should be called only once.
    pub(crate) fn new(cpuid: usize) -> Self {
        Self {
            plic: PLIC::new(cpuid),
        }
    }

    pub fn init(self: Pin<&mut Self>) {
        self.plic.set_threshold(0);

        // TODO: We should enable interrupts only when we register a handler.
        for i in 0..32 {
            self.plic.set_priority(i, 1);
            self.plic.enable_interrupt(i);
        }

        unsafe {
            sie::set_stimer();
            sie::set_sext();
        }
    }
}
