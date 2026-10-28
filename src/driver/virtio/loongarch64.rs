use alloc::boxed::Box;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use eonix_hal::fence::memory_barrier;
use eonix_hal::mm::ArchPhysAccess;
use eonix_log::println_warn;
use eonix_mm::address::PhysAccess;
use eonix_sync::Spin;
use virtio_drivers::device::blk::VirtIOBlk;
use virtio_drivers::transport::pci::bus::{
    ConfigurationAccess, DeviceFunction, PciRoot,
};
use virtio_drivers::transport::pci::PciTransport;
use virtio_drivers::transport::{DeviceType, Transport};

use super::virtio_blk::HAL;
use crate::kernel::block::BlockDevice;
use crate::kernel::constants::EIO;
use crate::kernel::pcie::{self, PCIDevice, PCIDriver, PciError, SegmentGroup};
use crate::kernel::task::block_on;
use crate::kernel::vfs::types::DeviceId;

impl ConfigurationAccess for &SegmentGroup {
    fn read_word(
        &self, device_function: DeviceFunction, register_offset: u8,
    ) -> u32 {
        let conf_space = self
            .get_conf_space(
                device_function.bus,
                device_function.device,
                device_function.function,
            )
            .expect("The given device function is out of range");

        let pointer = unsafe {
            // SAFETY: The base address is guaranteed to be valid by the PCI spec.
            ArchPhysAccess::as_ptr(conf_space.base + register_offset as usize)
        };

        memory_barrier();

        let value = unsafe {
            // SAFETY: The pointer is guaranteed to be valid and aligned for reading a u32 from.
            pointer.read_volatile()
        };

        memory_barrier();

        value
    }

    fn write_word(
        &mut self, device_function: DeviceFunction, register_offset: u8,
        data: u32,
    ) {
        let conf_space = self
            .get_conf_space(
                device_function.bus,
                device_function.device,
                device_function.function,
            )
            .expect("The given device function is out of range");

        let pointer = unsafe {
            // SAFETY: The base address is guaranteed to be valid by the PCI spec.
            ArchPhysAccess::as_ptr(conf_space.base + register_offset as usize)
        };

        memory_barrier();

        unsafe {
            // SAFETY: The pointer is guaranteed to be valid and aligned for writing a u32 to.
            pointer.write_volatile(data)
        };

        memory_barrier();
    }

    unsafe fn unsafe_clone(&self) -> Self {
        self
    }
}

struct VirtIODriver {
    disk_id: AtomicUsize,
}

#[async_trait]
impl PCIDriver for VirtIODriver {
    fn vendor_id(&self) -> u16 {
        0x1af4
    }

    fn device_id(&self) -> u16 {
        0x1001
    }

    async fn handle_device(
        &self, device: Arc<PCIDevice<'static>>,
    ) -> Result<(), PciError> {
        let transport = PciTransport::new::<HAL, _>(
            &mut PciRoot::new(device.segment_group()),
            DeviceFunction {
                bus: device.config_space().bus,
                device: device.config_space().device,
                function: device.config_space().function,
            },
        )
        .map_err(|err| {
            println_warn!(
                "Failed to create VirtIO transport for device {}:{}:{}: {}",
                device.config_space().bus,
                device.config_space().device,
                device.config_space().function,
                err
            );
            EIO
        })?;

        if transport.device_type() != DeviceType::Block {
            println_warn!(
                "Detected non-block VirtIO device ({:?}) in virtio block driver: {}:{}:{}",
                transport.device_type(),
                device.config_space().bus,
                device.config_space().device,
                device.config_space().function,
            );

            Err(EIO)?;
        }

        let virtio_block =
            VirtIOBlk::<HAL, _>::new(transport).map_err(|err| {
                println_warn!(
                    "Failed to initialize VirtIO Block device: {}",
                    err
                );
                EIO
            })?;

        let block_device = BlockDevice::register_disk(
            DeviceId::new(
                8,
                16 * self.disk_id.fetch_add(1, Ordering::AcqRel) as u16,
            ),
            2147483647, // TODO: Get size from device
            Arc::new(Spin::new(virtio_block)),
        )?;

        block_device.partprobe().await.map_err(|err| {
            println_warn!(
                "Failed to probe partitions for VirtIO Block device: {}",
                err
            );
            EIO
        })?;

        Ok(())
    }
}

pub fn init() {
    block_on(pcie::register_driver(VirtIODriver {
        disk_id: AtomicUsize::new(0),
    }))
    .expect("Failed to register VirtIO driver");
}
