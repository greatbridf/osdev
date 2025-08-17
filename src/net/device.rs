pub mod loopback;

use crate::prelude::KResult;
use alloc::{
    collections::{btree_map::BTreeMap, vec_deque::VecDeque},
    sync::Arc,
    vec,
    vec::Vec,
};
use eonix_log::println_debug;
use eonix_runtime::task::Task;
use eonix_sync::Mutex;

pub use smoltcp::phy::DeviceCapabilities;

pub type NetDevice = Arc<Mutex<dyn NetDev>>;
pub type Mac = [u8; 6];

pub static NETDEVS: Mutex<BTreeMap<&'static str, NetDevice>> = Mutex::new(BTreeMap::new());

pub fn register_netdev(name: &'static str, netdev: impl NetDev + 'static) -> KResult<()> {
    let netdev = Arc::new(Mutex::new(netdev));

    let mut netdevs = Task::block_on(NETDEVS.lock());
    netdevs.insert(name, netdev);
    drop(netdevs);

    Ok(())
}

pub fn get_netdev(name: &'static str) -> Option<NetDevice> {
    let netdevs = Task::block_on(NETDEVS.lock());
    netdevs.get(name).cloned()
}

#[derive(Debug, Clone, Copy)]
pub enum NetDevError {
    Unknown,
}

pub enum RxBuffer {
    VirtIOBuffer(virtio_drivers::device::net::RxBuffer),
    LoopBackBuffer(Vec<u8>),
}

impl RxBuffer {
    fn packet(&self) -> &[u8] {
        match self {
            RxBuffer::VirtIOBuffer(rx_buffer) => rx_buffer.packet(),
            RxBuffer::LoopBackBuffer(rx_buffer) => rx_buffer.as_slice(),
        }
    }

    pub fn into_virtio_buffer(self) -> Option<virtio_drivers::device::net::RxBuffer> {
        match self {
            RxBuffer::VirtIOBuffer(rx_buffer) => Some(rx_buffer),
            _ => None,
        }
    }
}

pub trait NetDev: Send {
    fn name(&self) -> &'static str;
    fn mac_addr(&self) -> Mac;
    fn caps(&self) -> DeviceCapabilities;

    fn can_receive(&self) -> bool;
    fn can_send(&self) -> bool;

    fn recv(&mut self) -> Result<RxBuffer, NetDevError>;
    fn recycle_rx_buffer(&mut self, rx_buffer: RxBuffer) -> Result<(), NetDevError>;
    fn send(&mut self, data: &[u8]) -> Result<(), NetDevError>;
}

impl smoltcp::phy::Device for dyn NetDev {
    type RxToken<'a>
        = RxToken
    where
        Self: 'a;

    type TxToken<'a>
        = TxToken<'a>
    where
        Self: 'a;

    fn receive(
        &mut self,
        _timestamp: smoltcp::time::Instant,
    ) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        if self.can_receive() && self.can_send() {
            let rx_buffer = self.recv().unwrap();
            Some((RxToken(rx_buffer), TxToken(self)))
        } else {
            None
        }
    }

    fn transmit(&mut self, _timestamp: smoltcp::time::Instant) -> Option<Self::TxToken<'_>> {
        if self.can_send() {
            Some(TxToken(self))
        } else {
            None
        }
    }

    fn capabilities(&self) -> DeviceCapabilities {
        self.caps()
    }
}

pub static USED_RX_BUFFERS: Mutex<VecDeque<RxBuffer>> = Mutex::new(VecDeque::new());

pub struct RxToken(RxBuffer);

impl smoltcp::phy::RxToken for RxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        let res = f(self.0.packet());
        Task::block_on(USED_RX_BUFFERS.lock()).push_back(self.0);
        res
    }
}

pub struct TxToken<'a>(&'a mut dyn NetDev);

impl smoltcp::phy::TxToken for TxToken<'_> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut tx_data = vec![0; len];
        let res = f(&mut tx_data);
        self.0.send(&tx_data).expect("Send packet failed");
        res
    }
}
