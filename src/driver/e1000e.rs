mod defs;
mod dev;
mod error;
mod rx_desc;
mod tx_desc;

use crate::kernel::constants::{EAGAIN, EFAULT, EINVAL, EIO};
use crate::kernel::interrupt::register_irq_handler;
use crate::kernel::mem::paging::{self, AllocZeroed};
use crate::kernel::mem::{AsMemoryBlock, PhysAccess};
use crate::kernel::pcie::{self, Header, PCIDevice, PCIDriver, PciError};
use crate::net::netdev;
use crate::prelude::*;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ptr::NonNull;
use dev::E1000eDev;
use eonix_hal::fence::memory_barrier;
use eonix_mm::address::{Addr, PAddr};
use eonix_sync::SpinIrq;
use paging::Page;

struct Driver {
    dev_id: u16,
}

impl PCIDriver for Driver {
    fn vendor_id(&self) -> u16 {
        0x8086
    }

    fn device_id(&self) -> u16 {
        self.dev_id
    }

    fn handle_device(&self, device: Arc<PCIDevice<'static>>) -> Result<(), PciError> {
        let Header::Endpoint(header) = device.header else {
            Err(EINVAL)?
        };

        let bar0 = header.bars[0];

        if bar0 & 0xf != 0 {
            Err(EINVAL)?;
        }

        device.enable_bus_mastering();

        let base = PAddr::from(bar0 as usize);

        let dev = E1000eDev::create(header.interrupt_line as usize, base).map_err(|_| EIO)?;
        dev.register().map_err(|_| EIO)?;
        dev.up().map_err(|_| EIO)?;

        Ok(())
    }
}

pub fn register_e1000e_driver() {
    let dev_ids = [0x100e, 0x10d3, 0x10ea, 0x153a];

    for id in dev_ids.into_iter() {
        pcie::register_driver(Driver { dev_id: id }).unwrap();
    }
}
