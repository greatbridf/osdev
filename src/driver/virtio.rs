mod virtio_blk;

#[cfg(not(target_arch = "riscv64"))]
compile_error!("VirtIO drivers are only supported on RISC-V architecture");

use crate::kernel::{
    block::{make_device, BlockDevice},
    mem::{AsMemoryBlock, MemoryBlock, Page},
};
use alloc::sync::Arc;
use core::num::NonZero;
use eonix_hal::{arch_exported::fdt::FDT, mm::ArchPhysAccess};
use eonix_log::{println_info, println_warn};
use eonix_mm::{
    address::{Addr, PAddr, PhysAccess},
    paging::PFN,
};
use eonix_runtime::task::Task;
use eonix_sync::Spin;
use virtio_drivers::{
    device::blk::VirtIOBlk,
    transport::{mmio::MmioTransport, Transport},
    Hal,
};

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
        size: usize,
    ) -> core::ptr::NonNull<u8> {
        MemoryBlock::new(NonZero::new(paddr).expect("paddr must be non-zero"), size).as_byte_ptr()
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

pub fn init_virtio_devices() {
    let mut disk_id = 0;
    for reg in FDT
        .all_nodes()
        .filter(|node| {
            node.compatible()
                .is_some_and(|compatible| compatible.all().any(|s| s == "virtio,mmio"))
        })
        .filter_map(|node| node.reg())
        .flatten()
    {
        let base = PAddr::from(reg.starting_address as usize);
        let size = reg.size.expect("Virtio device must have a size");

        let base = unsafe {
            // SAFETY: We get the base address from the FDT, which is guaranteed to be valid.
            ArchPhysAccess::as_ptr(base)
        };

        match unsafe { MmioTransport::new(base, size) } {
            Ok(transport) => match transport.device_type() {
                virtio_drivers::transport::DeviceType::Block => {
                    let block_device = VirtIOBlk::<HAL, _>::new(transport)
                        .expect("Failed to initialize VirtIO Block device");

                    let block_device = BlockDevice::register_disk(
                        make_device(8, 16 * disk_id),
                        2147483647,
                        Arc::new(Spin::new(block_device)),
                    )
                    .expect("Failed to register VirtIO Block device");

                    Task::block_on(block_device.partprobe())
                        .expect("Failed to probe partitions for VirtIO Block device");

                    disk_id += 1;
                }
                virtio_drivers::transport::DeviceType::Network => {
                    println_info!(
                        "Initializing Virtio Network device at {:?} with size {:#x}",
                        base,
                        size
                    );
                }
                virtio_drivers::transport::DeviceType::Console => {
                    println_info!(
                        "Initializing Virtio Console at {:?} with size {:#x}",
                        base,
                        size
                    );
                }
                virtio_drivers::transport::DeviceType::EntropySource => {
                    println_info!(
                        "Initializing Virtio Entropy Source at {:?} with size {:#x}",
                        base,
                        size
                    );
                }
                _ => {}
            },
            Err(err) => {
                println_warn!(
                    "Failed to initialize Virtio device at {:?} with size {:#x}: {}",
                    base,
                    size,
                    err
                );
            }
        }
    }
}
