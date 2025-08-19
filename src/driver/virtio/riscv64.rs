use crate::{
    driver::{self, virtio::VIRTIO_NET_NAME},
    kernel::{
        block::{make_device, BlockDevice},
        task::block_on,
    },
    net,
};
use driver::virtio::hal::HAL;

use alloc::sync::Arc;
use eonix_hal::{mm::ArchPhysAccess, platform::virtio_devs};
use eonix_log::{println_info, println_warn};
use eonix_mm::address::PhysAccess;
use eonix_sync::Spin;
use virtio_drivers::{
    device::{blk::VirtIOBlk, net::VirtIONet},
    transport::{mmio::MmioTransport, Transport},
};

pub fn init() {
    let mut disk_id = 0;

    for (base, size) in virtio_devs() {
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
