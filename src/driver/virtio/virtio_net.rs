use crate::driver::virtio::hal::HAL;
use crate::net::device::{Mac, NetDev, NetDevError, RxBuffer};
use eonix_log::println_debug;
use smoltcp::phy::{DeviceCapabilities, Medium};

use virtio_drivers::{
    device::net::{self, VirtIONet},
    transport::Transport,
};

pub static VIRTIO_NET_NAME: &'static str = "virtio_net";

impl<T, const QUEUE_SIZE: usize> NetDev for VirtIONet<HAL, T, QUEUE_SIZE>
where
    T: Transport + Send + Sync,
{
    fn name(&self) -> &'static str {
        VIRTIO_NET_NAME
    }

    fn mac_addr(&self) -> Mac {
        self.mac_address()
    }

    fn caps(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1514;
        caps.max_burst_size = None;
        caps.medium = Medium::Ethernet;
        caps
    }

    fn can_receive(&self) -> bool {
        self.can_recv()
    }

    fn can_send(&self) -> bool {
        self.can_send()
    }

    fn recv(&mut self) -> Result<RxBuffer, NetDevError> {
        self.receive().map_or_else(
            |_| Err(NetDevError::Unknown),
            |rx_buffer| Ok(RxBuffer::VirtIOBuffer(rx_buffer)),
        )
    }

    fn recycle_rx_buffer(&mut self, rx_buffer: RxBuffer) -> Result<(), NetDevError> {
        self.recycle_rx_buffer(rx_buffer.into_virtio_buffer().unwrap())
            .map_err(|_| NetDevError::Unknown)
    }

    fn send(&mut self, data: &[u8]) -> Result<(), NetDevError> {
        self.send(net::TxBuffer::from(data))
            .map_err(|_| NetDevError::Unknown)
    }
}
