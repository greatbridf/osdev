use alloc::vec::Vec;
use alloc::{boxed::Box, collections::VecDeque};
use smoltcp::phy::{ChecksumCapabilities, DeviceCapabilities, Medium};

use crate::net::device::{Mac, NetDev, NetDevError, RxBuffer};

pub struct Loopback {
    pub queue: VecDeque<Vec<u8>>,
}

impl Loopback {
    pub fn new() -> Loopback {
        Loopback {
            queue: VecDeque::new(),
        }
    }
}

pub static LOOPBACK_NAME: &'static str = "loopback";

impl NetDev for Loopback {
    fn name(&self) -> &'static str {
        LOOPBACK_NAME
    }

    fn mac_addr(&self) -> Mac {
        [0, 0, 0, 0, 0, 0]
    }

    fn can_receive(&self) -> bool {
        !self.queue.is_empty()
    }

    fn can_send(&self) -> bool {
        true
    }

    fn caps(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 65535;
        caps.max_burst_size = None;
        caps.medium = Medium::Ip;
        caps.checksum = ChecksumCapabilities::ignored();
        caps
    }

    fn recv(&mut self) -> Result<Box<dyn RxBuffer>, NetDevError> {
        self.queue
            .pop_back()
            .map(|rx_buffer| Box::new(rx_buffer) as _)
            .ok_or(NetDevError::Unknown)
    }

    fn send(&mut self, data: &[u8]) -> Result<(), NetDevError> {
        self.queue.push_back(Vec::from(data));
        Ok(())
    }
}

impl RxBuffer for Vec<u8> {
    fn packet(&self) -> &[u8] {
        self.as_slice()
    }
}
