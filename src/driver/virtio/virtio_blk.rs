use alloc::boxed::Box;

use async_trait::async_trait;
use eonix_hal::mm::ArchPhysAccess;
use eonix_mm::address::{Addr, PAddr, PhysAccess};
use eonix_mm::paging::{Folio as _, PFN};
use eonix_sync::Spin;
use virtio_drivers::device::blk::VirtIOBlk;
use virtio_drivers::transport::Transport;
use virtio_drivers::Hal;

use crate::io::Chunks;
use crate::kernel::block::{BlockDeviceRequest, BlockRequestQueue};
use crate::kernel::constants::EIO;
use crate::kernel::mem::Folio;
use crate::prelude::KResult;

pub struct HAL;

unsafe impl Hal for HAL {
    fn dma_alloc(
        pages: usize,
        _direction: virtio_drivers::BufferDirection,
    ) -> (virtio_drivers::PhysAddr, core::ptr::NonNull<u8>) {
        let page = Folio::alloc_at_least(pages);

        let ptr = page.get_ptr();
        let pfn = page.into_raw();

        (PAddr::from(pfn).addr(), ptr)
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
            Folio::from_raw(pfn);
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

#[async_trait]
impl<T> BlockRequestQueue for Spin<VirtIOBlk<HAL, T>>
where
    T: Transport + Send,
{
    fn max_request_pages(&self) -> u64 {
        1024
    }

    async fn submit<'a>(&'a self, req: BlockDeviceRequest<'a>) -> KResult<()> {
        match req {
            BlockDeviceRequest::Write {
                sector,
                count,
                buffer,
            } => {
                let mut dev = self.lock();
                for ((start, sectors), buffer_page) in
                    Chunks::new(sector as usize, count as usize, 8).zip(buffer.iter())
                {
                    let len = sectors * 512;
                    let pg = buffer_page.lock();

                    dev.write_blocks(start, &pg.as_bytes()[..len])
                        .map_err(|_| EIO)?;
                }
            }
            BlockDeviceRequest::Read {
                sector,
                count,
                buffer,
            } => {
                let mut dev = self.lock();
                for ((start, sectors), buffer_page) in
                    Chunks::new(sector as usize, count as usize, 8).zip(buffer.iter())
                {
                    let len = sectors * 512;
                    let mut pg = buffer_page.lock();

                    dev.read_blocks(start, &mut pg.as_bytes_mut()[..len])
                        .map_err(|_| EIO)?;
                }
            }
        }

        Ok(())
    }
}
