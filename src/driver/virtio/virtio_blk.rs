use super::HAL;
use crate::{
    io::Chunks,
    kernel::{
        block::{BlockDeviceRequest, BlockRequestQueue},
        constants::EIO,
        mem::AsMemoryBlock,
    },
    prelude::KResult,
};
use eonix_sync::Spin;
use virtio_drivers::{device::blk::VirtIOBlk, transport::mmio::MmioTransport};

impl BlockRequestQueue for Spin<VirtIOBlk<HAL, MmioTransport<'_>>> {
    fn max_request_pages(&self) -> u64 {
        1024
    }

    fn submit(&self, req: BlockDeviceRequest) -> KResult<()> {
        let mut dev = self.lock();
        for ((start, len), buffer_page) in
            Chunks::new(req.sector as usize, req.count as usize, 8).zip(req.buffer.iter())
        {
            let buffer = unsafe {
                // SAFETY: Pages in `req.buffer` are guaranteed to be exclusively owned by us.
                &mut buffer_page.as_memblk().as_bytes_mut()[..len as usize * 512]
            };

            dev.read_blocks(start, buffer).map_err(|_| EIO)?;
        }

        Ok(())
    }
}
