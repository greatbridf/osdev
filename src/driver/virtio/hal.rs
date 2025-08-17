use crate::kernel::mem::{AsMemoryBlock, Page};
use eonix_hal::mm::ArchPhysAccess;
use eonix_mm::{
    address::{Addr, PAddr, PhysAccess},
    paging::PFN,
};
use virtio_drivers::Hal;

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
