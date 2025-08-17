use core::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::driver::virtio::VIRTIO_NET_NAME;
use crate::kernel::constants::EADDRINUSE;
use crate::kernel::timer::Instant;
use crate::net::device::NetDevice;
use crate::net::socket::SocketType;
use crate::prelude::KResult;

use alloc::collections::btree_map::BTreeMap;
use alloc::collections::btree_set::BTreeSet;
use alloc::sync::Arc;
use alloc::vec;
use eonix_runtime::task::Task;
use eonix_sync::Mutex;
use smoltcp::phy::Medium;
use smoltcp::{
    iface::{Config, Interface, SocketHandle, SocketSet},
    socket::{tcp, udp},
    wire::{self, EthernetAddress, Ipv4Cidr},
};

pub static IFACES: Mutex<BTreeMap<&str, NetIface>> = Mutex::new(BTreeMap::new());

pub type NetIface = Arc<Mutex<Iface>>;

pub struct Iface {
    device: NetDevice,
    iface_inner: Interface,
    // Should distinguish TCP/UDP ports
    used_ports: BTreeSet<(SocketType, u16)>,
    sockets: SocketSet<'static>,
}

unsafe impl Send for Iface {}

const IP_LOCAL_PORT_START: u16 = 32768;
const IP_LOCAL_PORT_END: u16 = 60999;

const TCP_RX_BUF_LEN: usize = 65536;
const TCP_TX_BUF_LEN: usize = 65536;
const UDP_RX_BUF_LEN: usize = 65536;
const UDP_TX_BUF_LEN: usize = 65536;

impl Iface {
    pub fn new(device: NetDevice, ip_cidr: Ipv4Cidr, gateway: Option<Ipv4Addr>) -> Self {
        let iface_inner = {
            let mut device = Task::block_on(device.lock());
            let config = match device.caps().medium {
                Medium::Ethernet => Config::new(wire::HardwareAddress::Ethernet(EthernetAddress(
                    device.mac_addr(),
                ))),
                Medium::Ip => Config::new(wire::HardwareAddress::Ip),
            };
            let now = smoltcp::time::Instant::from_millis(Instant::now().to_millis() as i64);
            let mut iface = Interface::new(config, &mut *device, now);
            iface.update_ip_addrs(|ip_addrs| ip_addrs.push(wire::IpCidr::Ipv4(ip_cidr)).unwrap());

            if let Some(gateway) = gateway {
                iface.routes_mut().add_default_ipv4_route(gateway).unwrap();
            }

            iface
        };

        Self {
            device,
            iface_inner,
            used_ports: BTreeSet::new(),
            sockets: SocketSet::new(vec![]),
        }
    }

    pub fn iface_and_sockets(&mut self) -> (&mut Interface, &mut SocketSet<'static>) {
        (&mut self.iface_inner, &mut self.sockets)
    }

    fn new_tcp_socket(&mut self) -> SocketHandle {
        let rx_buffer = tcp::SocketBuffer::new(vec![0; TCP_RX_BUF_LEN]);
        let tx_buffer = tcp::SocketBuffer::new(vec![0; TCP_TX_BUF_LEN]);

        self.sockets.add(tcp::Socket::new(rx_buffer, tx_buffer))
    }

    fn new_udp_socket(&mut self) -> SocketHandle {
        let rx_buffer =
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 8], vec![0; UDP_RX_BUF_LEN]);
        let tx_buffer =
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 8], vec![0; UDP_TX_BUF_LEN]);

        self.sockets.add(udp::Socket::new(rx_buffer, tx_buffer))
    }

    pub fn remove_socket(&mut self, handle: SocketHandle, port: u16, socket_type: SocketType) {
        self.sockets.remove(handle);
        // FIXME: may many sockets use on port
        self.used_ports.remove(&(socket_type, port));
    }

    pub fn bind_socket(
        &mut self,
        bind_port: u16,
        socket_type: SocketType,
    ) -> KResult<(SocketAddr, SocketHandle)> {
        if self.used_ports.contains(&(socket_type, bind_port)) {
            return Err(EADDRINUSE);
        }

        let port = if bind_port == 0 {
            self.alloc_port(socket_type).ok_or(EADDRINUSE)?
        } else {
            self.used_ports.insert((socket_type, bind_port));
            bind_port
        };

        let socket_addr = SocketAddr::new(
            IpAddr::V4(self.iface_inner.ipv4_addr().expect("Set Ipv4 in construct")),
            port,
        );

        let socket_handle = match socket_type {
            SocketType::Tcp => self.new_tcp_socket(),
            SocketType::Udp => self.new_udp_socket(),
        };

        Ok((socket_addr, socket_handle))
    }

    fn alloc_port(&mut self, socket_type: SocketType) -> Option<u16> {
        // FIXME: more efficient way to allocate ports
        for port in IP_LOCAL_PORT_START..=IP_LOCAL_PORT_END {
            if !self.used_ports.contains(&(socket_type, port)) {
                self.used_ports.insert((socket_type, port));
                return Some(port);
            }
        }
        None
    }

    pub fn ipv4_addr(&self) -> Option<Ipv4Addr> {
        self.iface_inner.ipv4_addr()
    }

    pub fn poll(&mut self) {
        let mut device = Task::block_on(self.device.lock());
        let timestamp = smoltcp::time::Instant::from_millis(Instant::now().to_millis() as i64);

        self.iface_inner
            .poll(timestamp, &mut *device, &mut self.sockets);
    }
}

pub fn get_relate_iface(ip_addr: IpAddr) -> Option<NetIface> {
    let ifaces = Task::block_on(IFACES.lock());
    for iface in ifaces.values() {
        let iface_guard = Task::block_on(iface.lock());
        for cidr in iface_guard.iface_inner.ip_addrs() {
            if IpAddr::from(cidr.address()) == ip_addr {
                return Some(iface.clone());
            }
        }
    }

    None
}

pub fn get_ephemeral_iface(remote_addr: Option<IpAddr>) -> Option<NetIface> {
    let ifaces = Task::block_on(IFACES.lock());
    assert!(ifaces.len() > 0, "No network interfaces available");

    if let Some(remote_addr) = remote_addr {
        for iface in ifaces.values() {
            let iface_guard = Task::block_on(iface.lock());
            for cidr in iface_guard.iface_inner.ip_addrs() {
                if IpAddr::from(cidr.address()) == remote_addr {
                    return Some(iface.clone());
                }
            }
        }
    }

    // FIXME: Temporary use virtio-net as our default iface
    return ifaces.get(VIRTIO_NET_NAME).cloned();
}
