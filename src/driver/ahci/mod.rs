use crate::{
    fs::procfs,
    kernel::{
        block::{make_device, BlockDevice},
        interrupt::register_irq_handler,
    },
    prelude::*,
};

use alloc::{format, sync::Arc};
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

pub struct BitsIterator {
    data: u32,
    n: u32,
}

impl BitsIterator {
    fn new(data: u32) -> Self {
        Self { data, n: 0 }
    }
}

impl Iterator for BitsIterator {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.n == 32 {
            return None;
        }

        let have: bool = self.data & 1 != 0;
        self.data >>= 1;
        self.n += 1;

        if have {
            Some(self.n - 1)
        } else {
            self.next()
        }
    }
}

fn vread<T: Sized + Copy>(refval: *const T) -> T {
    unsafe { refval.read_volatile() }
}

fn vwrite<T: Sized + Copy>(refval: *mut T, val: T) {
    unsafe { refval.write_volatile(val) }
}

struct Device {
    control_base: usize,
    control: AdapterControl,
    // TODO: impl Drop to free pci device
    pcidev: *mut pci_device,
    /// # Lock
    /// Might be accessed from irq handler, use with `lock_irq()`
    ports: Spin<[Option<Arc<AdapterPort>>; 32]>,
}

/// # Safety
/// `pcidev` is never accessed from Rust code
/// TODO!!!: place *mut pci_device in a safe wrapper
unsafe impl Send for Device {}
unsafe impl Sync for Device {}

impl Device {
    fn probe_ports(&self) -> KResult<()> {
        for nport in self.control.implemented_ports() {
            let port = Arc::new(AdapterPort::new(self.control_base, nport));
            if !port.status_ok() {
                continue;
            }

            self.ports.lock_irq()[nport as usize] = Some(port.clone());
            if let Err(e) = (|| -> KResult<()> {
                port.init()?;

                {
                    let port = port.clone();
                    let name = format!("ahci-p{}-stats", port.nport);
                    procfs::populate_root(name.into_bytes().into(), move |buffer| {
                        writeln!(buffer, "{:?}", port.stats.lock().as_ref()).map_err(|_| EIO)
                    })?;
                }

                let port = BlockDevice::register_disk(
                    make_device(8, nport * 16),
                    2147483647, // TODO: get size from device
                    port,
                )?;

                port.partprobe()?;

                Ok(())
            })() {
                self.ports.lock_irq()[nport as usize] = None;
                println_warn!("probe port {nport} failed with {e}");
            }
        }

        Ok(())
    }

    fn handle_interrupt(&self) {
        // Safety
        // `self.ports` is accessed inside irq handler
        let ports = self.ports.lock();
        for nport in self.control.pending_interrupts() {
            if let None = ports[nport as usize] {
                println_warn!("port {nport} not found");
                continue;
            }

            let port = ports[nport as usize].as_ref().unwrap();
            let status = vread(port.interrupt_status());

            if status & PORT_IS_ERROR != 0 {
                println_warn!("port {nport} SATA error");
                continue;
            }

            debug_assert!(status & PORT_IS_DHRS != 0);
            vwrite(port.interrupt_status(), PORT_IS_DHRS);

            self.control.clear_interrupt(nport);

            port.handle_interrupt();
        }
    }
}

impl Device {
    pub fn new(pcidev: *mut pci_device) -> KResult<Arc<Self>> {
        let base = unsafe { *(*pcidev).header_type0() }.bars[PCI_REG_ABAR];
        let irqno = unsafe { *(*pcidev).header_type0() }.interrupt_line;

        // use MMIO
        if base & 0xf != 0 {
            return Err(EIO);
        }

        let device = Arc::new(Device {
            control_base: base as usize,
            control: AdapterControl::new(base as usize),
            pcidev,
            ports: Spin::new([const { None }; 32]),
        });

        device.control.enable_interrupts();

        let device_irq = device.clone();
        register_irq_handler(irqno as i32, move || device_irq.handle_interrupt())?;

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
        }
        Err(e) => -(e as i32),
    }
}

pub fn register_ahci_driver() {
    let ret = unsafe { pci::register_driver_r(VENDOR_INTEL, DEVICE_AHCI, Some(probe_device)) };

    assert_eq!(ret, 0);
}
