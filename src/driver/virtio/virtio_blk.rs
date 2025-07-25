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
        match req {
            BlockDeviceRequest::Write {
                sector,
                count,
                buffer,
            } => {
                let mut dev = self.lock();
                for ((start, len), buffer_page) in
                    Chunks::new(sector as usize, count as usize, 8).zip(buffer.iter())
                {
                    let buffer = unsafe {
                        // SAFETY: Pages in `req.buffer` are guaranteed to be exclusively owned by us.
                        &buffer_page.as_memblk().as_bytes()[..len as usize * 512]
                    };

                    dev.write_blocks(start, buffer).map_err(|_| EIO)?;
                }
            }
            BlockDeviceRequest::Read {
                sector,
                count,
                buffer,
            } => {
                let mut dev = self.lock();
                for ((start, len), buffer_page) in
                    Chunks::new(sector as usize, count as usize, 8).zip(buffer.iter())
                {
                    let buffer = unsafe {
                        // SAFETY: Pages in `req.buffer` are guaranteed to be exclusively owned by us.
                        &mut buffer_page.as_memblk().as_bytes_mut()[..len as usize * 512]
                    };

                    dev.read_blocks(start, buffer).map_err(|_| EIO)?;
                }
            }
        }

        Ok(())
    }
}
