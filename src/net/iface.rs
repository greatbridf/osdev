use core::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::kernel::constants::EADDRINUSE;
use crate::kernel::timer::Instant;
use crate::net::device::NetDevice;
use crate::net::socket::tcp::TcpSocket;
use crate::prelude::KResult;

use alloc::sync::Arc;
use alloc::vec;
use alloc::{collections::btree_set::BTreeSet, vec::Vec};
use eonix_runtime::task::Task;
use eonix_sync::Mutex;
use smoltcp::{
    iface::{Config, Interface, SocketHandle, SocketSet},
    socket::tcp,
    wire::{self, EthernetAddress, Ipv4Cidr},
};

pub static IFACES: Mutex<Vec<NetIface>> = Mutex::new(Vec::new());

pub type NetIface = Arc<Mutex<Iface>>;

pub struct Iface {
    device: NetDevice,
    iface_inner: Interface,
    // Should distinguish TCP/UDP ports
    used_ports: BTreeSet<u16>,
    sockets: SocketSet<'static>,
}

const IP_LOCAL_PORT_START: u16 = 32768;
const IP_LOCAL_PORT_END: u16 = 60999;

unsafe impl Send for Iface {}

impl Iface {
    pub fn new(
        device: NetDevice,
        ether_addr: EthernetAddress,
        ip_cidr: Ipv4Cidr,
        gateway: Ipv4Addr,
    ) -> Self {
        let iface_inner = {
            let config = Config::new(wire::HardwareAddress::Ethernet(ether_addr));
            let now = smoltcp::time::Instant::from_millis(Instant::now().to_millis() as i64);
            let mut device = device.lock();
            let mut iface = Interface::new(config, &mut *device, now);
            iface.update_ip_addrs(|ip_addrs| ip_addrs.push(wire::IpCidr::Ipv4(ip_cidr)).unwrap());
            iface.routes_mut().add_default_ipv4_route(gateway).unwrap();
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
        let rx_buffer = tcp::SocketBuffer::new(vec![0; 1024]);
        let tx_buffer = tcp::SocketBuffer::new(vec![0; 1024]);

        self.sockets.add(tcp::Socket::new(rx_buffer, tx_buffer))
    }

    pub fn remove_tcp_socket(&mut self, socket: &TcpSocket) {
        self.sockets
            .remove(socket.handle().expect("Should have a socket handle"));

        if let Some(socket_addr) = socket.local_addr() {
            self.used_ports.remove(&socket_addr.port());
        }
    }

    pub fn bind_tcp_socket(&mut self, bind_port: u16) -> KResult<(SocketAddr, SocketHandle)> {
        if self.used_ports.contains(&bind_port) {
            return Err(EADDRINUSE);
        }

        let port = if bind_port == 0 {
            self.alloc_port().ok_or(EADDRINUSE)?
        } else {
            bind_port
        };

        let socket_addr = SocketAddr::new(
            IpAddr::V4(self.iface_inner.ipv4_addr().expect("Set Ipv4 in construct")),
            port,
        );

        let socket_handle = self.new_tcp_socket();

        Ok((socket_addr, socket_handle))
    }

    fn alloc_port(&mut self) -> Option<u16> {
        // FIXME: more efficient way to allocate ports
        for port in IP_LOCAL_PORT_START..=IP_LOCAL_PORT_END {
            if !self.used_ports.contains(&port) {
                self.used_ports.insert(port);
                return Some(port);
            }
        }
        None
    }

    pub fn poll(&mut self) {
        let mut device = self.device.lock();
        let timestamp = smoltcp::time::Instant::from_millis(Instant::now().to_millis() as i64);

        self.iface_inner
            .poll(timestamp, &mut *device, &mut self.sockets);
    }
}

pub fn get_relate_iface(ip_addr: IpAddr) -> Option<NetIface> {
    let ifaces = Task::block_on(IFACES.lock());
    for iface in ifaces.iter() {
        let iface_guard = Task::block_on(iface.lock());
        for cidr in iface_guard.iface_inner.ip_addrs() {
            if IpAddr::from(cidr.address()) == ip_addr {
                return Some(iface.clone());
            }
        }
    }
    None
}

pub fn get_ephemeral_iface(_remote_addr: Option<IpAddr>) -> Option<NetIface> {
    let ifaces = Task::block_on(IFACES.lock());
    assert!(ifaces.len() > 0, "No network interfaces available");

    for iface in ifaces.iter() {
        // FIXME: This is a temporary solution, we should select the best interface based on some criteria.
        return Some(iface.clone());
    }

    None
}
