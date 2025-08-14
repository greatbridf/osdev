pub mod loopback;

use crate::prelude::KResult;
use alloc::{boxed::Box, sync::Arc, vec, vec::Vec};
use eonix_sync::Spin;

pub use smoltcp::phy::DeviceCapabilities;

pub type NetDevice = Arc<Spin<dyn NetDev>>;
pub type Mac = [u8; 6];

pub static NETDEVS: Spin<Vec<NetDevice>> = Spin::new(Vec::new());

pub fn register_netdev(netdev: impl NetDev + 'static) -> KResult<NetDevice> {
    let netdev = Arc::new(Spin::new(netdev));

    let mut netdevs = NETDEVS.lock();
    netdevs.push(netdev.clone());
    drop(netdevs);

    Ok(netdev)
}

#[derive(Debug, Clone, Copy)]
pub enum NetDevError {
    Unknown,
}

pub trait RxBuffer {
    fn packet(&self) -> &[u8];
}

pub trait NetDev: Send {
    fn name(&self) -> &'static str;
    fn mac_addr(&self) -> Mac;
    fn caps(&self) -> DeviceCapabilities;

    fn can_receive(&self) -> bool;
    fn can_send(&self) -> bool;

    fn recv(&mut self) -> Result<Box<dyn RxBuffer>, NetDevError>;
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

pub struct RxToken(Box<dyn RxBuffer>);

impl smoltcp::phy::RxToken for RxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(self.0.packet())
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
