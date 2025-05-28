use crate::{
    fs::procfs,
    io::Buffer as _,
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
use core::ptr::NonNull;
use defs::*;
use eonix_mm::address::{AddrOps as _, PAddr};
use eonix_sync::SpinIrq as _;
use port::AdapterPort;

pub(self) use register::Register;

mod command;
mod command_table;
mod control;
mod defs;
mod port;
mod register;
pub(self) mod slot;
mod stats;

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

struct Device<'a> {
    control_base: PAddr,
    control: AdapterControl,
    // TODO: impl Drop to free pci device
    pcidev: NonNull<pci_device>,
    /// # Lock
    /// Might be accessed from irq handler, use with `lock_irq()`
    ports: Spin<[Option<Arc<AdapterPort<'a>>>; 32]>,
}

/// # Safety
/// `pcidev` is never accessed from Rust code
/// TODO!!!: place *mut pci_device in a safe wrapper
unsafe impl Send for Device<'_> {}
unsafe impl Sync for Device<'_> {}

impl Device<'_> {
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
            let status = port.interrupt_status().read_once();

            if status & PORT_IS_ERROR != 0 {
                println_warn!("port {nport} SATA error");
                continue;
            }

            debug_assert!(status & PORT_IS_DHRS != 0);
            port.interrupt_status().write_once(PORT_IS_DHRS);

            self.control.clear_interrupt(nport);

            port.handle_interrupt();
        }
    }
}

impl Device<'static> {
    pub fn new(pcidev: NonNull<pci_device>) -> KResult<Arc<Self>> {
        let base =
            PAddr::from(unsafe { *pcidev.as_ref().header_type0() }.bars[PCI_REG_ABAR] as usize);
        let irqno = unsafe { *pcidev.as_ref().header_type0() }.interrupt_line;

        // use MMIO
        if !base.is_aligned_to(16) {
            return Err(EIO);
        }

        let device = Arc::new(Device {
            control_base: base,
            control: AdapterControl::new(base),
            pcidev,
            ports: Spin::new([const { None }; 32]),
        });

        device.control.enable_interrupts();

        let device_irq = device.clone();
        register_irq_handler(irqno as i32, move || device_irq.handle_interrupt())?;

        device.probe_ports()?;

        Ok(device)
    }

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
                        port.print_stats(&mut buffer.get_writer())
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
}

unsafe extern "C" fn probe_device(pcidev: *mut pci_device) -> i32 {
    match Device::new(NonNull::new(pcidev).expect("NULL `pci_device` pointer")) {
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
