use crate::{
    driver::virtio::{hal::HAL, VIRTIO_NET_NAME},
    kernel::block::{make_device, BlockDevice},
    net,
};

use alloc::{sync::Arc, vec::Vec};
use eonix_hal::arch_exported::fdt::FDT;
use eonix_hal::mm::ArchPhysAccess;
use eonix_log::{println_info, println_warn};
use eonix_mm::address::{PAddr, PhysAccess};
use eonix_runtime::task::Task;
use eonix_sync::Spin;
use virtio_drivers::{
    device::{blk::VirtIOBlk, net::VirtIONet},
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

                    Task::block_on(block_device.partprobe())
                        .expect("Failed to probe partitions for VirtIO Block device");

                    disk_id += 1;
                }
                virtio_drivers::transport::DeviceType::Network => {
                    const NET_QUEUE_SIZE: usize = 64;
                    const NET_BUF_LEN: usize = 2048;
                    let net_device =
                        VirtIONet::<HAL, _, NET_QUEUE_SIZE>::new(transport, NET_BUF_LEN)
                            .expect("Failed to initialize VirtIO Net device");
                    net::register_netdev(VIRTIO_NET_NAME, net_device)
                        .expect("Failed to register VirtIO Net device");
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
