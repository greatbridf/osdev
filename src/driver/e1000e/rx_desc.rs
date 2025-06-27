use super::{dev::Registers, error::E1000eError};
use crate::{
    driver::e1000e::defs::{REG_RDH, REG_RDT, RXD_STAT_DD},
    kernel::mem::{AsMemoryBlock, Page},
};
use core::{cell::UnsafeCell, ptr::NonNull};
use eonix_mm::{
    address::{Addr, PAddr},
    paging::{PAGE_SIZE, PFN},
};

#[repr(C)]
struct RxDescriptor {
    buffer: u64,
    length: u16,
    checksum: u16,
    status: u8,
    errors: u8,
    vlan: u16,
}

/// Represents a table of RX descriptors for the E1000E network driver.
///
/// TODO: impl `Drop` for RxDescriptorTable to free the pages.
pub struct RxDescriptorTable {
    tail: u32,
    len: u32,
    table: NonNull<[UnsafeCell<RxDescriptor>]>,
    _table_page: Page,
}

unsafe impl Send for RxDescriptorTable {}
unsafe impl Sync for RxDescriptorTable {}

pub struct RxBuffer {
    buffer: NonNull<[u8]>,
    _page: Page,
}

unsafe impl Send for RxBuffer {}
unsafe impl Sync for RxBuffer {}

impl RxDescriptor {
    const fn zeroed() -> Self {
        Self {
            buffer: 0,
            length: 0,
            checksum: 0,
            status: 0,
            errors: 0,
            vlan: 0,
        }
    }

    fn zero(&mut self) {
        *self = Self::zeroed();
    }

    fn set_buffer(&mut self, page: Page) {
        self.zero();
        self.buffer = PAddr::from(page.pfn()).addr() as u64;

        page.into_raw();
    }

    fn take_buffer(&mut self) -> RxBuffer {
        let len = self.length as usize;
        let page = unsafe {
            // SAFETY: The buffer is guaranteed to be valid as it was set before.
            Page::from_raw(PFN::from(PAddr::from(self.buffer as usize)))
        };

        self.zero();

        let buffer_ptr = page.as_memblk().as_byte_ptr();
        RxBuffer {
            buffer: NonNull::slice_from_raw_parts(buffer_ptr, len),
            _page: page,
        }
    }
}

impl RxDescriptorTable {
    /// Maximum number of descriptors in the RX descriptor table.
    pub const MAX_DESCS: u32 = 65536;

    pub const fn tail(&self) -> u32 {
        self.tail
    }

    pub const fn next_tail(&self) -> u32 {
        (self.tail + 1) % self.len
    }

    pub fn new(len: u32) -> Result<Self, E1000eError> {
        if len > Self::MAX_DESCS {
            return Err(E1000eError::TooManyDescriptors);
        }

        let size = len as usize * size_of::<RxDescriptor>();
        let page_count = size / PAGE_SIZE;

        let table_page = Page::alloc_at_least(page_count);
        let page_ptr = table_page.as_memblk().as_byte_ptr();
        let table_ptr = NonNull::slice_from_raw_parts(page_ptr.cast(), len as usize);

        unsafe {
            let table: &mut [UnsafeCell<RxDescriptor>] = table_ptr.clone().as_mut();

            for entry in table.iter_mut().map(UnsafeCell::get_mut) {
                entry.set_buffer(Page::alloc());
            }
        }

        Ok(Self {
            tail: len - 1,
            len,
            table: table_ptr,
            _table_page: table_page,
        })
    }

    pub fn base(&self) -> PAddr {
        PAddr::from(self._table_page.pfn())
    }

    pub fn byte_len(&self) -> u32 {
        self.len * size_of::<RxDescriptor>() as u32
    }

    pub fn has_data(&self, regs: &Registers) -> bool {
        let head = regs.read(REG_RDH);
        let next_tail = self.next_tail();
        next_tail != head
    }

    pub fn received<'b>(
        &mut self,
        regs: &'b Registers,
    ) -> impl Iterator<Item = RxBuffer> + use<'_, 'b> {
        struct Received<'a, 'b> {
            table: &'a mut RxDescriptorTable,
            regs: &'b Registers,
        }

        impl Iterator for Received<'_, '_> {
            type Item = RxBuffer;

            fn next(&mut self) -> Option<Self::Item> {
                if !self.table.has_data(self.regs) {
                    return None; // No new packets
                }

                let next_tail = self.table.next_tail();

                let descriptor = unsafe {
                    // SAFETY: The descriptor at position `tail` belongs to us.
                    &mut *self.table.table.as_ref()[next_tail as usize].get()
                };

                debug_assert!(descriptor.status & RXD_STAT_DD != 0, "Descriptor not ready");
                let rx_buffer = descriptor.take_buffer();
                descriptor.set_buffer(Page::alloc());

                self.table.tail = next_tail;
                self.regs.write(REG_RDT, next_tail);

                Some(rx_buffer)
            }
        }

        Received { table: self, regs }
    }
}

impl RxBuffer {
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            // SAFETY: The buffer is guaranteed to be valid as it was set before.
            self.buffer.as_ref()
        }
    }
}
