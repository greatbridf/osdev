use super::virtio_blk::HAL;
use crate::kernel::{
    block::{make_device, BlockDevice},
    task::block_on,
};
use alloc::{sync::Arc, vec::Vec};
use eonix_hal::arch_exported::fdt::FDT;
use eonix_hal::mm::ArchPhysAccess;
use eonix_log::{println_info, println_warn};
use eonix_mm::address::{PAddr, PhysAccess};
use eonix_sync::Spin;
use virtio_drivers::{
    device::blk::VirtIOBlk,
    transport::{mmio::MmioTransport, Transport},
};

pub fn init() {
    let mut disk_id = 0;
    let mut virtio_devices: Vec<_> = FDT
        .all_nodes()
        .filter(|node| {
            node.compatible()
                .is_some_and(|compatible| compatible.all().any(|s| s == "virtio,mmio"))
        })
        .filter_map(|node| node.reg())
        .flatten()
        .collect();
    virtio_devices.sort_by_key(|reg| reg.starting_address);

    for reg in virtio_devices {
        let base = PAddr::from(reg.starting_address as usize);
        let size = reg.size.expect("Virtio device must have a size");

        let base = unsafe {
            // SAFETY: We get the base address from the FDT, which is guaranteed to be valid.
            ArchPhysAccess::as_ptr(base)
        };

        match unsafe { MmioTransport::new(base, size) } {
            Ok(transport) => match transport.device_type() {
                virtio_drivers::transport::DeviceType::Block => {
                    let block_device = VirtIOBlk::<HAL, _>::new(transport)
                        .expect("Failed to initialize VirtIO Block device");

                    let block_device = BlockDevice::register_disk(
                        make_device(8, 16 * disk_id),
                        2147483647,
                        Arc::new(Spin::new(block_device)),
                    )
                    .expect("Failed to register VirtIO Block device");

                    block_on(block_device.partprobe())
                        .expect("Failed to probe partitions for VirtIO Block device");

                    disk_id += 1;
                }
                virtio_drivers::transport::DeviceType::Network => {
                    println_info!(
                        "Initializing Virtio Network device at {:?} with size {:#x}",
                        base,
                        size
                    );
                }
                virtio_drivers::transport::DeviceType::Console => {
                    println_info!(
                        "Initializing Virtio Console at {:?} with size {:#x}",
                        base,
                        size
                    );
                }
                virtio_drivers::transport::DeviceType::EntropySource => {
                    println_info!(
                        "Initializing Virtio Entropy Source at {:?} with size {:#x}",
                        base,
                        size
                    );
                }
                _ => {}
            },
            Err(err) => {
                println_warn!(
                    "Failed to initialize Virtio device at {:?} with size {:#x}: {}",
                    base,
                    size,
                    err
                );
            }
        }
    }
}
