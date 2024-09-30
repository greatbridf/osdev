use crate::{
    kernel::block::{make_device, BlockDevice},
    prelude::*,
};

use alloc::sync::Arc;
use bindings::{
    kernel::hw::pci::{self, pci_device},
    EIO,
};
use control::AdapterControl;
use defs::*;
use port::AdapterPort;

mod command;
mod control;
mod defs;
mod port;

fn vread<T: Sized + Copy>(refval: &T) -> T {
    unsafe { core::ptr::read_volatile(refval) }
}

fn vwrite<T: Sized + Copy>(refval: &mut T, val: T) {
    unsafe { core::ptr::write_volatile(refval, val) }
}

fn spinwait_clear(refval: &u32, mask: u32) -> KResult<()> {
    const SPINWAIT_MAX: usize = 1000;

    let mut spins = 0;
    while vread(refval) & mask != 0 {
        if spins == SPINWAIT_MAX {
            return Err(EIO);
        }

        spins += 1;
    }

    Ok(())
}

fn spinwait_set(refval: &u32, mask: u32) -> KResult<()> {
    const SPINWAIT_MAX: usize = 1000;

    let mut spins = 0;
    while vread(refval) & mask != mask {
        if spins == SPINWAIT_MAX {
            return Err(EIO);
        }

        spins += 1;
    }

    Ok(())
}

struct Device<'lt, 'port> {
    control_base: usize,
    control: &'lt mut AdapterControl,
    // TODO: impl Drop to free pci device
    pcidev: *mut pci_device,
    ports: Vec<Option<Arc<Mutex<AdapterPort<'port>>>>>,
}

impl<'lt, 'port: 'static> Device<'lt, 'port> {
    fn probe_ports(&mut self) -> KResult<()> {
        for nport in self.control.implemented_ports() {
            let mut port = AdapterPort::<'port>::new(self.control_base, nport);

            if !port.status_ok() {
                continue;
            }

            port.init()?;

            let port = Arc::new(Mutex::new(port));

            self.ports[nport as usize] = Some(port.clone());

            let port = BlockDevice::register_disk(
                make_device(8, nport * 16),
                2147483647, // TODO: get size from device
                port,
            )?;

            port.partprobe()?;
        }

        Ok(())
    }
}

impl<'lt: 'static, 'port: 'static> Device<'lt, 'port> {
    pub fn new(pcidev: *mut pci_device) -> KResult<Self> {
        let base = unsafe { *(*pcidev).header_type0() }.bars[PCI_REG_ABAR];

        // use MMIO
        if base & 0xf != 0 {
            return Err(EIO);
        }

        let mut ports = Vec::with_capacity(32);
        ports.resize_with(32, || None);

        let mut device = Device {
            control_base: base as usize,
            control: AdapterControl::new(base as usize),
            pcidev,
            ports,
        };

        device.control.enable_interrupts();
        device.probe_ports()?;

        Ok(device)
    }
}

unsafe extern "C" fn probe_device(pcidev: *mut pci_device) -> i32 {
    match Device::new(pcidev) {
        Ok(device) => {
            // TODO!!!: save device to pci_device
            Box::leak(Box::new(device));
            0
        },
        Err(e) => -(e as i32),
    }
}

pub fn register_ahci_driver() {
    let ret = unsafe {
        pci::register_driver_r(VENDOR_INTEL, DEVICE_AHCI, Some(probe_device))
    };

    assert_eq!(ret, 0);
}
