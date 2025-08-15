pub mod device;
pub mod iface;
pub mod netdev;
pub mod socket;

pub use device::register_netdev;

use alloc::sync::Arc;
use core::time::Duration;
use eonix_log::println_warn;
use eonix_runtime::{run::FutureRun, scheduler::Scheduler, task::Task};
use eonix_sync::{Mutex, Spin};
use smoltcp::wire::{Ipv4Address, Ipv4Cidr};

use crate::{
    driver::virtio::VIRTIO_NET_NAME,
    kernel::{task::KernelStack, timer::sleep},
    net::{
        device::{
            loopback::{Loopback, LOOPBACK_NAME},
            NETDEVS,
        },
        iface::{Iface, IFACES},
    },
    prelude::KResult,
};

const VIRTIO_ADDRESS: Ipv4Address = Ipv4Address::new(10, 0, 2, 15);
const VIRTIO_ADDRESS_PREFIX_LEN: u8 = 24;
const VIRTIO_GATEWAY: Ipv4Address = Ipv4Address::new(10, 0, 2, 2);
const LOOPBACK_ADDRESS: Ipv4Address = Ipv4Address::new(127, 0, 0, 1);
const LOOPBACK_ADDRESS_PREFIX_LEN: u8 = 8;

pub fn init() -> KResult<()> {
    let mut ifaces = Task::block_on(IFACES.lock());
    let mut netdevs = NETDEVS.lock();

    netdevs.push(Arc::new(Spin::new(Loopback::new())));

    for netdev in netdevs.iter() {
        let name = netdev.lock().name();

        if name == VIRTIO_NET_NAME {
            let virtio_iface = Iface::new(
                netdev.clone(),
                Ipv4Cidr::new(VIRTIO_ADDRESS, VIRTIO_ADDRESS_PREFIX_LEN),
                Some(VIRTIO_GATEWAY),
            );
            ifaces.insert(name, Arc::new(Mutex::new(virtio_iface)));
        } else if name == LOOPBACK_NAME {
            let loopback_iface = Iface::new(
                netdev.clone(),
                Ipv4Cidr::new(LOOPBACK_ADDRESS, LOOPBACK_ADDRESS_PREFIX_LEN),
                None,
            );
            ifaces.insert(name, Arc::new(Mutex::new(loopback_iface)));
        } else {
            println_warn!("Currently only virtio_net is supported");
        }
    }

    drop(ifaces);
    drop(netdevs);

    Scheduler::get().spawn::<KernelStack, _>(FutureRun::new(ifaces_poll()));

    Ok(())
}

// Temporary spawn a task to poll network interfaces
// Better register soft irq handler
pub async fn ifaces_poll() {
    loop {
        let ifaces = IFACES.lock().await;
        for iface in ifaces.values() {
            let mut iface_guard = iface.lock().await;
            iface_guard.poll();
        }
        drop(ifaces);

        sleep(Duration::from_millis(100)).await;
    }
}
