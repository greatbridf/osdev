use super::{BitsIterator, GHC_IE};
use crate::kernel::mem::PhysAccess as _;
use core::ptr::NonNull;
use eonix_hal::fence::memory_barrier;
use eonix_mm::address::PAddr;

/// An `AdapterControl` is an HBA device Global Host Control block
///
/// # Access
///
/// All reads and writes to this struct is volatile
///
#[allow(dead_code)]
#[repr(C)]
struct AdapterControlData {
    capabilities: u32,
    global_host_control: u32,
    interrupt_status: u32,
    ports_implemented: u32,
    version: u32,
    command_completion_coalescing_control: u32,
    command_completion_coalescing_ports: u32,
    enclosure_management_location: u32,
    enclosure_management_control: u32,
    host_capabilities_extended: u32,
    bios_handoff_control_status: u32,

    _reserved: [u8; 116],
    vendor: [u8; 96],
}

#[allow(dead_code)]
const CONTROL_CAP: usize = 0;
const CONTROL_GHC: usize = 1;
const CONTROL_IS: usize = 2;
const CONTROL_PI: usize = 3;

pub struct AdapterControl {
    control_data: NonNull<u32>,
}

/// # Safety
/// At the same time, exactly one instance of this struct may exist.
unsafe impl Send for AdapterControl {}

impl AdapterControl {
    pub fn new(addr: PAddr) -> Self {
        Self {
            control_data: unsafe { addr.as_ptr() },
        }
    }
}

impl AdapterControl {
    fn read(&self, off: usize) -> u32 {
        unsafe { self.control_data.offset(off as isize).read_volatile() }
    }

    fn write(&self, off: usize, value: u32) {
        unsafe { self.control_data.offset(off as isize).write_volatile(value) }
    }

    pub fn enable_interrupts(&self) {
        let ghc = self.read(CONTROL_GHC);
        self.write(CONTROL_GHC, ghc | GHC_IE);
        memory_barrier();
    }

    pub fn implemented_ports(&self) -> BitsIterator {
        BitsIterator::new(self.read(CONTROL_PI))
    }

    pub fn pending_interrupts(&self) -> BitsIterator {
        BitsIterator::new(self.read(CONTROL_IS))
    }

    pub fn clear_interrupt(&self, no: u32) {
        self.write(CONTROL_IS, 1 << no);
        memory_barrier();
    }
}
