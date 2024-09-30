use crate::{
    kernel::mem::phys::{NoCachePP, PhysPtr},
    prelude::*,
};

use super::{vread, vwrite, GHC_IE};

/// An `AdapterControl` is an HBA device Global Host Control block
///
/// # Access
///
/// All reads and writes to this struct is volatile
///
#[repr(C)]
pub struct AdapterControl {
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

impl AdapterControl {
    pub fn new<'lt>(addr: usize) -> &'lt mut Self {
        NoCachePP::new(addr).as_mut()
    }
}

impl AdapterControl {
    pub fn enable_interrupts(&mut self) {
        let ghc = vread(&self.global_host_control);
        vwrite(&mut self.global_host_control, ghc | GHC_IE);
    }

    pub fn implemented_ports(&self) -> ImplementedPortsIter {
        ImplementedPortsIter::new(vread(&self.ports_implemented))
    }
}

pub struct ImplementedPortsIter {
    ports: u32,
    n: u32,
}

impl ImplementedPortsIter {
    fn new(ports: u32) -> Self {
        Self { ports, n: 0 }
    }
}

impl Iterator for ImplementedPortsIter {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.n == 32 {
            return None;
        }

        let have: bool = self.ports & 1 != 0;
        self.ports >>= 1;
        self.n += 1;

        if have {
            Some(self.n - 1)
        } else {
            self.next()
        }
    }
}
