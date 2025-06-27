use super::{LinkId, LinkState, NetError};
use crate::kernel::timer::{sleep, Ticks};
use alloc::{collections::btree_map::BTreeMap, sync::Arc, vec, vec::Vec};
use core::ops::{Deref, DerefMut};
use eonix_log::println_trace;
use eonix_sync::Spin;
use smoltcp::{
    iface::{Config, Interface, PollIngressSingleResult, PollResult, SocketSet},
    phy::Device,
    socket::Socket,
    time::Instant,
    wire::{EthernetAddress, IpAddress},
};

static NETDEVS: Spin<Vec<Arc<dyn AnyNetDevice>>> = Spin::new(Vec::new());
static ADDRESS_DEVS: Spin<BTreeMap<IpAddress, Arc<dyn BindSocket>>> = Spin::new(BTreeMap::new());

pub trait AnyNetDevice: Send + Sync {
    fn id(&self) -> LinkId;
}

pub trait PhyDevice: Send + Sync {
    type Device: Device;

    fn device(&self) -> impl DerefMut<Target = Self::Device>;
    fn state(&self) -> LinkState;

    fn up(&self) -> Result<(), NetError>;
    fn down(&self) -> Result<(), NetError>;
}

pub trait BindSocket: Send + Sync {
    fn bind(&self, address: IpAddress, socket: Socket) -> Result<(), NetError>;
}

pub struct NetDeviceInner<P>
where
    P: PhyDevice,
{
    id: LinkId,
    link_state: Spin<LinkState>,

    phy: P,
    interface: Spin<Interface>,
    sockets: Spin<SocketSet<'static>>,
    addresses: Spin<Vec<IpAddress>>,
}

#[derive(Clone)]
pub struct NetDevice<P>
where
    P: PhyDevice,
{
    _inner: Arc<NetDeviceInner<P>>,
}

impl<P> Deref for NetDevice<P>
where
    P: PhyDevice,
{
    type Target = NetDeviceInner<P>;

    fn deref(&self) -> &Self::Target {
        &self._inner
    }
}

impl<P> AnyNetDevice for NetDeviceInner<P>
where
    P: PhyDevice,
{
    fn id(&self) -> LinkId {
        self.id
    }
}

impl<P> NetDevice<P>
where
    P: PhyDevice + 'static,
{
    pub fn register(&self) -> Result<(), NetError> {
        let mut netdevs = NETDEVS.lock();
        if let Some(_) = netdevs.iter().find(|dev| dev.id() == self.id) {
            return Err(NetError::AlreadyRegistered);
        }
        netdevs.push(self._inner.clone());
        Ok(())
    }
}

impl<P> NetDevice<P>
where
    P: PhyDevice,
{
    pub fn new(phy: P) -> Self {
        let state = phy.state();
        let config = Config::new(EthernetAddress::from_bytes(state.mac.as_ref()).into());

        let interface = Interface::new(
            config,
            &mut *phy.device(),
            Instant::ZERO + Ticks::since_boot().into(),
        );

        Self {
            _inner: Arc::new(NetDeviceInner {
                id: LinkId::new(),
                link_state: Spin::new(state),
                phy,
                interface: Spin::new(interface),
                sockets: Spin::new(SocketSet::new(vec![])),
                addresses: Spin::new(vec![]),
            }),
        }
    }

    pub fn update_link_state(&self, link_state: LinkState) {
        *self.link_state.lock() = link_state;
    }

    pub fn up(&self) -> Result<(), NetError> {
        self.phy.up()
    }

    pub fn down(&self) -> Result<(), NetError> {
        self.phy.down()
    }

    pub async fn worker(&self) {
        loop {
            const POLL_IN_MAX: usize = 4096;

            let mut phy = self.phy.device();
            let mut interface = self.interface.lock();
            let mut sockets = self.sockets.lock();
            let now = Instant::ZERO + Ticks::since_boot().into();

            let mut result = PollResult::None;

            for _ in 0..POLL_IN_MAX {
                match interface.poll_ingress_single(now, &mut *phy, &mut sockets) {
                    PollIngressSingleResult::None => break,
                    PollIngressSingleResult::PacketProcessed => {}
                    PollIngressSingleResult::SocketStateChanged => {
                        result = PollResult::SocketStateChanged;
                    }
                }
            }

            if let PollResult::SocketStateChanged =
                interface.poll_egress(now, &mut *phy, &mut sockets)
            {
                result = PollResult::SocketStateChanged;
            }

            if let PollResult::None = result {
                let Some(duration_before_next_poll) = interface.poll_delay(now, &sockets) else {
                    continue;
                };

                sleep(duration_before_next_poll.into()).await;
                continue;
            }

            println_trace!("trace_net", "Socket state changed");

            for (_, socket) in sockets.iter_mut() {
                match socket {
                    Socket::Udp(socket) => todo!(),
                    Socket::Tcp(socket) => {
                        let mut active = false;
                        if socket.is_active() && !active {
                            println_trace!("trace_net", "TCP socket is active");
                        } else if !socket.is_active() && active {
                            println_trace!("trace_net", "TCP socket is no longer active");
                        }
                        active = socket.is_active();

                        if socket.can_recv() {
                            socket
                                .recv(|buffer| {
                                    #[allow(unused_variables)]
                                    let string = str::from_utf8(buffer);
                                    println_trace!(
                                        "trace_net",
                                        "Received data on TCP socket: {string:#?}"
                                    );

                                    (buffer.len(), ())
                                })
                                .expect("Failed to receive data on TCP socket");
                        }

                        if socket.can_send() {
                            socket
                                .send(|buffer| {
                                    let data =
                                        b"GET / HTTP/1.1\r\nHost: self.greatbridf.com\r\n\r\n";
                                    buffer[..data.len()].copy_from_slice(data);

                                    (data.len(), ())
                                })
                                .expect("Failed to send data on TCP socket");
                        }
                    }
                    Socket::Dhcpv4(socket) => match socket.poll() {
                        Some(smoltcp::socket::dhcpv4::Event::Configured(config)) => {
                            println_trace!("trace_net", "DHCP configured: {:?}", config);

                            interface.update_ip_addrs(|addrs| {
                                addrs.clear();
                                addrs.push(config.address.into()).unwrap();
                            });

                            if let Some(router) = config.router {
                                interface
                                    .routes_mut()
                                    .add_default_ipv4_route(router)
                                    .unwrap();
                            } else {
                                interface.routes_mut().remove_default_ipv4_route();
                            }

                            #[allow(unused_variables)]
                            for dns in config.dns_servers {
                                // TODO: export DNS servers to `/etc/resolv.conf`
                                println_trace!("trace_net", "Found DNS server: {:?}", dns);
                            }
                        }
                        Some(smoltcp::socket::dhcpv4::Event::Deconfigured) => {
                            println_trace!("trace_net", "DHCP deconfigured");
                        }
                        None => {}
                    },
                    Socket::Dns(socket) => todo!(),
                }
            }
        }

        // sockets
        //     .get_mut::<smoltcp::socket::tcp::Socket>(tcp)
        //     .connect(
        //         iface.context(),
        //         (IpAddress::from(Ipv4Addr::new(212, 50, 246, 131)), 80),
        //         56789,
        //     )
        //     .expect("Failed to connect TCP socket");
    }
}
