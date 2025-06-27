use eonix_mm::{
    address::{Addr, PAddr, PRange},
    paging::PAGE_SIZE,
};

use crate::{
    driver::e1000e::defs::{REG_TDT, TXD_STAT_DD},
    kernel::mem::{AsMemoryBlock, Page},
};
use core::{cell::UnsafeCell, ptr::NonNull};

use super::{
    defs::{REG_TDH, TXD_CMD_EOP, TXD_CMD_IFCS, TXD_CMD_RS},
    dev::Registers,
    error::E1000eError,
};

#[repr(C)]
struct TxDescriptor {
    buffer: u64,
    length: u16,
    cso: u8, // Checksum offset
    cmd: u8,
    status: u8,
    css: u8, // Checksum start
    vlan: u16,
}

/// Represents a table of RX descriptors for the E1000E network driver.
pub struct TxDescriptorTable {
    tail: u32,
    len: u32,
    table: NonNull<[UnsafeCell<TxDescriptor>]>,
    _table_page: Page,
}

unsafe impl Send for TxDescriptorTable {}
unsafe impl Sync for TxDescriptorTable {}

impl TxDescriptor {
    const fn zeroed() -> Self {
        Self {
            buffer: 0,
            length: 0,
            cso: 0,
            cmd: 0,
            status: 0,
            css: 0,
            vlan: 0,
        }
    }

    fn zero(&mut self) {
        *self = Self::zeroed();
    }

    fn set(&mut self, data: PRange) -> Result<(), E1000eError> {
        if data.len() > PAGE_SIZE as usize {
            return Err(E1000eError::TooBigDataSize);
        }

        self.buffer = data.start().addr() as u64;
        self.length = data.len() as u16;
        self.cmd = TXD_CMD_EOP | TXD_CMD_IFCS | TXD_CMD_RS; // End of packet, Insert FCS, Report status.
        self.status = 0; // Clear status.

        Ok(())
    }
}

impl TxDescriptorTable {
    /// Maximum number of descriptors in the TX descriptor table.
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

        let size = len as usize * size_of::<TxDescriptor>();
        let page_count = size / PAGE_SIZE;

        let table_page = Page::alloc_at_least(page_count);
        let page_ptr = table_page.as_memblk().as_byte_ptr();
        let table_ptr = NonNull::slice_from_raw_parts(page_ptr.cast(), len as usize);

        unsafe {
            let table: &mut [UnsafeCell<TxDescriptor>] = table_ptr.clone().as_mut();

            for entry in table.iter_mut().map(UnsafeCell::get_mut) {
                entry.zero();
                entry.status = TXD_STAT_DD; // Mark descriptor as ready.
            }
        }

        Ok(Self {
            tail: 0,
            len,
            table: NonNull::slice_from_raw_parts(page_ptr.cast(), len as usize),
            _table_page: table_page,
        })
    }

    pub fn base(&self) -> PAddr {
        PAddr::from(self._table_page.pfn())
    }

    pub fn byte_len(&self) -> u32 {
        self.len * size_of::<TxDescriptor>() as u32
    }

    pub fn can_send(&self, regs: &Registers) -> bool {
        let head = regs.read(REG_TDH);
        let next_tail = self.next_tail();
        head != next_tail
    }

    pub fn send(&mut self, regs: &Registers, data: PRange) -> Result<(), E1000eError> {
        if !self.can_send(regs) {
            return Err(E1000eError::NoFreeDescriptor);
        }

        let next_tail = self.next_tail();

        let descriptor = unsafe {
            // SAFETY: The descriptor at position `tail` belongs to us.
            &mut *self.table.as_ref()[self.tail() as usize].get()
        };

        debug_assert!(descriptor.status & TXD_STAT_DD != 0, "Descriptor not ready");

        descriptor.zero();
        descriptor.set(data)?;

        self.tail = next_tail;
        regs.write(REG_TDT, next_tail);

        while regs.read(REG_TDH) != next_tail {
            core::hint::spin_loop();
        }

        debug_assert!(
            descriptor.status & TXD_STAT_DD != 0,
            "Descriptor not ready after sending"
        );

        Ok(())
    }
}
