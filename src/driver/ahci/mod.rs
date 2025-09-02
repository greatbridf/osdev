use crate::{
    fs::procfs,
    io::Buffer as _,
    kernel::{
        block::BlockDevice,
        constants::{EINVAL, EIO},
        interrupt::register_irq_handler,
        pcie::{self, Header, PCIDevice, PCIDriver, PciError},
        vfs::types::DeviceId,
    },
    prelude::*,
};
use alloc::{format, sync::Arc};
use async_trait::async_trait;
use control::AdapterControl;
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

pub struct AHCIDriver {
    devices: Spin<Vec<Arc<Device<'static>>>>,
}

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
    _pcidev: Arc<PCIDevice<'static>>,
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
    async fn probe_port(&self, port: Arc<AdapterPort<'static>>) -> KResult<()> {
        port.init().await?;

        {
            let port = port.clone();
            let name = format!("ahci-p{}-stats", port.nport);
            procfs::populate_root(name.into_bytes().into(), move |buffer| {
                port.print_stats(&mut buffer.get_writer())
            })
            .await;
        }

        let port = BlockDevice::register_disk(
            DeviceId::new(8, port.nport as u16 * 16),
            2147483647, // TODO: get size from device
            port,
        )?;

        port.partprobe().await?;

        Ok(())
    }

    async fn probe_ports(&self) -> KResult<()> {
        for nport in self.control.implemented_ports() {
            let port = Arc::new(AdapterPort::new(self.control_base, nport));
            if !port.status_ok() {
                continue;
            }

            self.ports.lock_irq()[nport as usize] = Some(port.clone());
            if let Err(e) = self.probe_port(port).await {
                self.ports.lock_irq()[nport as usize] = None;
                println_warn!("probe port {nport} failed with {e}");
            }
        }

        Ok(())
    }
}

impl AHCIDriver {
    pub fn new() -> Self {
        Self {
            devices: Spin::new(Vec::new()),
        }
    }
}

#[async_trait]
impl PCIDriver for AHCIDriver {
    fn vendor_id(&self) -> u16 {
        VENDOR_INTEL
    }

    fn device_id(&self) -> u16 {
        DEVICE_AHCI
    }

    async fn handle_device(&self, pcidev: Arc<PCIDevice<'static>>) -> Result<(), PciError> {
        let Header::Endpoint(header) = pcidev.header else {
            Err(EINVAL)?
        };

        let bar5 = header.bars().iter().nth(PCI_REG_ABAR).ok_or(EINVAL)?.get();
        let base = match bar5 {
            pcie::Bar::MemoryMapped32 {
                base: Some(base), ..
            } => PAddr::from(base.get() as usize),

            pcie::Bar::MemoryMapped64 {
                base: Some(base), ..
            } => PAddr::from(base.get() as usize),

            _ => todo!("Unsupported BAR type"),
        };

        let irqno = header.interrupt_line;

        // use MMIO
        if !base.is_aligned_to(16) {
            Err(EIO)?;
        }

        let device = Arc::new(Device {
            control_base: base,
            control: AdapterControl::new(base),
            _pcidev: pcidev,
            ports: Spin::new([const { None }; 32]),
        });

        device.control.enable_interrupts();

        let device_irq = device.clone();
        register_irq_handler(irqno as i32, move || device_irq.handle_interrupt())?;

        device.probe_ports().await?;

        self.devices.lock().push(device);

        Ok(())
    }
}

pub async fn register_ahci_driver() {
    pcie::register_driver(AHCIDriver::new())
        .await
        .expect("Register ahci driver failed");
}
