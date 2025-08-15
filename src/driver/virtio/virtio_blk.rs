use crate::{
    io::Chunks,
    kernel::{
        block::{BlockDeviceRequest, BlockRequestQueue},
        constants::EIO,
        mem::{AsMemoryBlock, Page},
    },
    prelude::KResult,
};
use eonix_hal::mm::ArchPhysAccess;
use eonix_mm::{
    address::{Addr, PAddr, PhysAccess},
    paging::PFN,
};
use eonix_sync::Spin;
use virtio_drivers::{device::blk::VirtIOBlk, transport::Transport, Hal};

pub struct HAL;

unsafe impl Hal for HAL {
    fn dma_alloc(
        pages: usize,
        _direction: virtio_drivers::BufferDirection,
    ) -> (virtio_drivers::PhysAddr, core::ptr::NonNull<u8>) {
        let page = Page::alloc_at_least(pages);

        let paddr = page.start().addr();
        let ptr = page.as_memblk().as_byte_ptr();
        page.into_raw();

        (paddr, ptr)
    }

    unsafe fn dma_dealloc(
        paddr: virtio_drivers::PhysAddr,
        _vaddr: core::ptr::NonNull<u8>,
        _pages: usize,
    ) -> i32 {
        let pfn = PFN::from(PAddr::from(paddr));

        unsafe {
            // SAFETY: The caller ensures that the pfn corresponds to a valid
            //         page allocated by `dma_alloc`.
            Page::from_raw(pfn);
        }

        0
    }

    unsafe fn mmio_phys_to_virt(
        paddr: virtio_drivers::PhysAddr,
        _size: usize,
    ) -> core::ptr::NonNull<u8> {
        unsafe { ArchPhysAccess::as_ptr(PAddr::from(paddr)) }
    }

    unsafe fn share(
        buffer: core::ptr::NonNull<[u8]>,
        _direction: virtio_drivers::BufferDirection,
    ) -> virtio_drivers::PhysAddr {
        let paddr = unsafe {
            // SAFETY: The caller ensures that the buffer is valid.
            ArchPhysAccess::from_ptr(buffer.cast::<u8>())
        };

        paddr.addr()
    }

    unsafe fn unshare(
        _paddr: virtio_drivers::PhysAddr,
        _buffer: core::ptr::NonNull<[u8]>,
        _direction: virtio_drivers::BufferDirection,
    ) {
    }
}

impl<T> BlockRequestQueue for Spin<VirtIOBlk<HAL, T>>
where
    T: Transport + Send,
{
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
