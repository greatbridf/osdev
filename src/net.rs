pub mod device;
pub mod iface;
pub mod netdev;
pub mod socket;

pub use device::register_netdev;

use alloc::{sync::Arc, vec::Vec};
use core::time::Duration;
use eonix_log::println_warn;
use eonix_runtime::{run::FutureRun, scheduler::Scheduler, task::Task};
use eonix_sync::Mutex;
use smoltcp::wire::{EthernetAddress, Ipv4Address, Ipv4Cidr};

use crate::{
    kernel::{task::KernelStack, timer::sleep},
    net::{
        device::NETDEVS,
        iface::{Iface, IFACES},
    },
    prelude::KResult,
};

const VIRTIO_ADDRESS: Ipv4Address = Ipv4Address::new(10, 0, 2, 15);
const VIRTIO_ADDRESS_PREFIX_LEN: u8 = 24;
const VIRTIO_GATEWAY: Ipv4Address = Ipv4Address::new(10, 0, 2, 2);

pub fn init() -> KResult<()> {
    let netdevs = NETDEVS.lock();

    // We don't supported dynamic registering network devices yet,
    assert!(netdevs.len() > 0, "No network devices registered");

    let mut ifaces = Vec::new();

    for netdev in netdevs.values() {
        let netdev_guard = netdev.lock();
        let name = netdev_guard.name();
        let ether_addr = netdev_guard.mac_addr();
        drop(netdev_guard);

        if name == "virtio_net" {
            let iface = Iface::new(
                netdev.clone(),
                EthernetAddress(ether_addr),
                Ipv4Cidr::new(VIRTIO_ADDRESS, VIRTIO_ADDRESS_PREFIX_LEN),
                VIRTIO_GATEWAY,
            );

            ifaces.push(Arc::new(Mutex::new(iface)));
        } else {
            println_warn!("Currently only virtio_net is supported");
        }
    }

    drop(netdevs);

    Task::block_on(IFACES.lock()).extend(ifaces);

    // Temporary scheduler task to poll network interfaces
    // Better register a soft irq
    Scheduler::get().spawn::<KernelStack, _>(FutureRun::new(async {
        loop {
            let ifaces = IFACES.lock().await;
            for iface in ifaces.iter() {
                let mut iface_guard = iface.lock().await;
                iface_guard.poll();
            }
            drop(ifaces);

            sleep(Duration::from_millis(100)).await;
        }
    }));

    Ok(())
}
