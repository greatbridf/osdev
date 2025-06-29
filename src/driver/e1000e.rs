use crate::kernel::constants::{EAGAIN, EFAULT, EINVAL, EIO};
use crate::kernel::interrupt::register_irq_handler;
use crate::kernel::mem::paging::{self, AllocZeroed};
use crate::kernel::mem::{AsMemoryBlock, PhysAccess};
use crate::kernel::pcie::{self, Header, PCIDevice, PCIDriver, PciError};
use crate::net::netdev;
use crate::prelude::*;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ptr::NonNull;
use eonix_hal::fence::memory_barrier;
use eonix_mm::address::{Addr, PAddr};
use eonix_sync::SpinIrq;
use paging::Page;

mod defs;

#[repr(C)]
struct RxDescriptor {
    buffer: u64,
    length: u16,
    checksum: u16,
    status: u8,
    errors: u8,
    vlan: u16,
}

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

const RX_DESC_SIZE: usize = 32;
const TX_DESC_SIZE: usize = 32;

struct Registers(NonNull<()>);

unsafe impl Send for Registers {}
unsafe impl Sync for Registers {}

struct E1000eDev {
    irq_no: usize,

    mac: netdev::Mac,
    status: netdev::LinkStatus,
    speed: netdev::LinkSpeed,
    id: u32,

    regs: Registers,
    rt_desc_page: Page,
    rx_head: Option<u32>,
    rx_tail: Option<u32>,
    tx_tail: Option<u32>,

    rx_buffers: Option<Box<Vec<Page>>>,
    tx_buffers: Option<Box<Vec<Page>>>,
}

fn test(val: u32, bit: u32) -> bool {
    (val & bit) == bit
}

struct PrintableBytes<'a>(&'a [u8]);

impl core::fmt::Debug for PrintableBytes<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "PrintableBytes {{")?;
        for chunk in self.0.chunks(16) {
            for &byte in chunk {
                write!(f, "{byte:#2x} ")?;
            }
            write!(f, "\n")?;
        }
        write!(f, "}}")?;

        Ok(())
    }
}

impl Registers {
    fn new(base: PAddr) -> Self {
        Self(unsafe { base.as_ptr() })
    }

    fn read(&self, offset: u32) -> u32 {
        let retval = unsafe {
            // SAFETY: The offset is within the bounds of the device's memory-mapped registers.
            self.0.byte_offset(offset as isize).cast().read_volatile()
        };
        memory_barrier();
        retval
    }

    fn write(&self, offset: u32, value: u32) {
        unsafe {
            // SAFETY: The offset is within the bounds of the device's memory-mapped registers.
            self.0
                .byte_offset(offset as isize)
                .cast()
                .write_volatile(value);
        }
        memory_barrier();
    }

    fn read_as<T: Copy>(&self, offset: u32) -> T {
        let retval = unsafe {
            // SAFETY: The offset is within the bounds of the device's memory-mapped registers.
            self.0.byte_offset(offset as isize).cast().read_volatile()
        };
        memory_barrier();
        retval
    }
}

impl netdev::Netdev for E1000eDev {
    fn mac(&self) -> netdev::Mac {
        self.mac
    }

    fn link_status(&self) -> netdev::LinkStatus {
        self.status
    }

    fn link_speed(&self) -> netdev::LinkSpeed {
        self.speed
    }

    fn id(&self) -> u32 {
        self.id
    }

    fn up(&mut self) -> Result<(), u32> {
        let ctrl = self.regs.read(defs::REG_CTRL);
        let status = self.regs.read(defs::REG_STAT);

        // check link up
        if !test(ctrl, defs::CTRL_SLU) || !test(status, defs::STAT_LU) {
            return Err(EIO);
        }

        // auto negotiation of speed
        match status & defs::STAT_SPEED_MASK {
            defs::STAT_SPEED_10M => self.speed = netdev::LinkSpeed::Speed10M,
            defs::STAT_SPEED_100M => self.speed = netdev::LinkSpeed::Speed100M,
            defs::STAT_SPEED_1000M => self.speed = netdev::LinkSpeed::Speed1000M,
            _ => return Err(EINVAL),
        }

        // clear multicast table
        for i in (0..128).step_by(4) {
            self.regs.write(defs::REG_MTA + i, 0);
        }

        self.clear_stats()?;

        // setup interrupt handler
        let device = netdev::get_netdev(self.id).unwrap();
        register_irq_handler(self.irq_no as _, move || {
            device.lock().fire().unwrap();
        })?;

        // enable interrupts
        self.regs
            .write(defs::REG_IMS, defs::ICR_NORMAL | defs::ICR_UP);

        // read to clear any pending interrupts
        self.regs.read(defs::REG_ICR);

        self.setup_rx()?;
        self.setup_tx()?;

        self.status = netdev::LinkStatus::Up;

        Ok(())
    }

    fn fire(&mut self) -> Result<(), u32> {
        let cause = self.regs.read(defs::REG_ICR);
        if !test(cause, defs::ICR_INT) {
            return Ok(());
        }

        loop {
            let tail = self.rx_tail.ok_or(EIO)?;
            let next_tail = (tail + 1) % RX_DESC_SIZE as u32;

            if next_tail == self.regs.read(defs::REG_RDH) {
                break;
            }

            let ref mut desc = self.rx_desc_table()[next_tail as usize];
            if !test(desc.status as u32, defs::RXD_STAT_DD as u32) {
                Err(EIO)?;
            }

            desc.status = 0;
            let len = desc.length as usize;

            let buffers = self.rx_buffers.as_mut().ok_or(EIO)?;
            let data = unsafe {
                // SAFETY: No one could be writing to the buffer at this point.
                &buffers[next_tail as usize].as_memblk().as_bytes()[..len]
            };

            println_debug!("e1000e: received {len} bytes, {:?}", PrintableBytes(data));
            self.rx_tail = Some(next_tail);
        }

        Ok(())
    }

    fn send(&mut self, buf: &[u8]) -> Result<(), u32> {
        let tail = self.tx_tail.ok_or(EIO)?;
        let head = self.regs.read(defs::REG_TDH);
        let next_tail = (tail + 1) % TX_DESC_SIZE as u32;

        if next_tail == head {
            return Err(EAGAIN);
        }

        let ref mut desc = self.tx_desc_table()[tail as usize];
        if !test(desc.status as u32, defs::TXD_STAT_DD as u32) {
            return Err(EIO);
        }

        let buffer_page = Page::alloc();
        if buf.len() > buffer_page.len() {
            return Err(EFAULT);
        }

        unsafe {
            // SAFETY: We are the only one writing to this memory block.
            buffer_page.as_memblk().as_bytes_mut()[..buf.len()].copy_from_slice(buf);
        }

        desc.buffer = PAddr::from(buffer_page.pfn()).addr() as u64;
        desc.length = buf.len() as u16;
        desc.cmd = defs::TXD_CMD_EOP | defs::TXD_CMD_IFCS | defs::TXD_CMD_RS;
        desc.status = 0;

        self.tx_tail = Some(next_tail);
        self.regs.write(defs::REG_TDT, next_tail);

        // TODO: check if the packets are sent and update self.tx_head state

        Ok(())
    }
}

impl E1000eDev {
    fn setup_rx(&mut self) -> Result<(), u32> {
        if !self.rx_head.is_none() || !self.rx_tail.is_none() {
            return Err(EINVAL);
        }

        let addr = PAddr::from(self.rt_desc_page.pfn()).addr();

        self.regs.write(defs::REG_RDBAL, addr as u32);
        self.regs.write(defs::REG_RDBAH, (addr >> 32) as u32);

        self.regs.write(
            defs::REG_RDLEN,
            (RX_DESC_SIZE * size_of::<RxDescriptor>()) as u32,
        );

        self.regs.write(defs::REG_RDH, 0);
        self.regs.write(defs::REG_RDT, RX_DESC_SIZE as u32 - 1);

        self.rx_head = Some(0);
        self.rx_tail = Some(RX_DESC_SIZE as u32 - 1);

        self.regs.write(
            defs::REG_RCTL,
            defs::RCTL_EN
                | defs::RCTL_MPE
                | defs::RCTL_LPE
                | defs::RCTL_LBM_NO
                | defs::RCTL_DTYP_LEGACY
                | defs::RCTL_BAM
                | defs::RCTL_BSIZE_8192
                | defs::RCTL_SECRC,
        );

        Ok(())
    }

    fn setup_tx(&mut self) -> Result<(), u32> {
        if !self.tx_tail.is_none() {
            return Err(EINVAL);
        }

        let addr = PAddr::from(self.rt_desc_page.pfn()).addr() + 0x200;

        self.regs.write(defs::REG_TDBAL, addr as u32);
        self.regs.write(defs::REG_TDBAH, (addr >> 32) as u32);

        self.regs.write(
            defs::REG_TDLEN,
            (TX_DESC_SIZE * size_of::<TxDescriptor>()) as u32,
        );

        self.regs.write(defs::REG_TDH, 0);
        self.regs.write(defs::REG_TDT, 0);

        self.tx_tail = Some(0);

        self.regs.write(
            defs::REG_TCTL,
            defs::TCTL_EN
                | defs::TCTL_PSP
                | (15 << defs::TCTL_CT_SHIFT)
                | (64 << defs::TCTL_COLD_SHIFT)
                | defs::TCTL_RTLC,
        );

        Ok(())
    }

    fn reset(&self) -> Result<(), u32> {
        // disable interrupts so we won't mess things up
        self.regs.write(defs::REG_IMC, 0xffffffff);

        let ctrl = self.regs.read(defs::REG_CTRL);
        self.regs.write(defs::REG_CTRL, ctrl | defs::CTRL_GIOD);

        while self.regs.read(defs::REG_STAT) & defs::STAT_GIOE != 0 {
            // wait for link up
        }

        let ctrl = self.regs.read(defs::REG_CTRL);
        self.regs.write(defs::REG_CTRL, ctrl | defs::CTRL_RST);

        while self.regs.read(defs::REG_CTRL) & defs::CTRL_RST != 0 {
            // wait for reset
        }

        // disable interrupts again
        self.regs.write(defs::REG_IMC, 0xffffffff);

        Ok(())
    }

    fn clear_stats(&self) -> Result<(), u32> {
        self.regs.write(defs::REG_COLC, 0);
        self.regs.write(defs::REG_GPRC, 0);
        self.regs.write(defs::REG_MPRC, 0);
        self.regs.write(defs::REG_GPTC, 0);
        self.regs.write(defs::REG_GORCL, 0);
        self.regs.write(defs::REG_GORCH, 0);
        self.regs.write(defs::REG_GOTCL, 0);
        self.regs.write(defs::REG_GOTCH, 0);
        Ok(())
    }

    pub fn new(base: PAddr, irq_no: usize) -> Result<Self, u32> {
        let page = Page::zeroed();

        let mut dev = Self {
            irq_no,
            mac: [0; 6],
            status: netdev::LinkStatus::Down,
            speed: netdev::LinkSpeed::SpeedUnknown,
            id: netdev::alloc_id(),
            regs: Registers::new(base),
            rt_desc_page: page,
            rx_head: None,
            rx_tail: None,
            tx_tail: None,
            rx_buffers: None,
            tx_buffers: None,
        };

        dev.reset()?;

        dev.mac = dev.regs.read_as(0x5400);
        dev.tx_buffers = Some(Box::new(Vec::with_capacity(TX_DESC_SIZE)));

        let mut rx_buffers = Box::new(Vec::with_capacity(RX_DESC_SIZE));

        for index in 0..RX_DESC_SIZE {
            let page = Page::alloc_order(2);

            let ref mut desc = dev.rx_desc_table()[index];
            desc.buffer = PAddr::from(page.pfn()).addr() as u64;
            desc.status = 0;

            rx_buffers.push(page);
        }

        for index in 0..TX_DESC_SIZE {
            let ref mut desc = dev.tx_desc_table()[index];
            desc.status = defs::TXD_STAT_DD;
        }

        dev.rx_buffers = Some(rx_buffers);

        Ok(dev)
    }

    fn rx_desc_table(&self) -> &mut [RxDescriptor; RX_DESC_SIZE] {
        unsafe {
            // SAFETY: TODO
            self.rt_desc_page.as_memblk().as_ptr().as_mut()
        }
    }

    fn tx_desc_table(&self) -> &mut [TxDescriptor; TX_DESC_SIZE] {
        let (_, right) = self.rt_desc_page.as_memblk().split_at(0x200);
        unsafe {
            // SAFETY: TODO
            right.as_ptr().as_mut()
        }
    }
}

impl Drop for E1000eDev {
    fn drop(&mut self) {
        assert_eq!(self.status, netdev::LinkStatus::Down);

        if let Some(_) = self.rx_buffers.take() {}

        // TODO: we should wait until all packets are sent
        if let Some(_) = self.tx_buffers.take() {}

        let _ = self.rt_desc_page;
    }
}

struct Driver {
    dev_id: u16,
}

impl PCIDriver for Driver {
    fn vendor_id(&self) -> u16 {
        0x8086
    }

    fn device_id(&self) -> u16 {
        self.dev_id
    }

    fn handle_device(&self, device: Arc<PCIDevice<'static>>) -> Result<(), PciError> {
        let Header::Endpoint(header) = device.header else {
            Err(EINVAL)?
        };

        let bar0 = header.bars().iter().next().ok_or(EINVAL)?.get();
        let base = match bar0 {
            pcie::Bar::MemoryMapped32 {
                base: Some(base), ..
            } => PAddr::from(base.get() as usize),

            pcie::Bar::MemoryMapped64 {
                base: Some(base), ..
            } => PAddr::from(base.get() as usize),

            _ => todo!("Unsupported BAR type"),
        };

        let e1000e = E1000eDev::new(base, header.interrupt_line as usize)?;

        let dev = netdev::register_netdev(e1000e)?;
        dev.lock_irq().up()?;

        Ok(())
    }
}

pub fn register_e1000e_driver() {
    let dev_ids = [0x100e, 0x10d3, 0x10ea, 0x153a];

    for id in dev_ids.into_iter() {
        pcie::register_driver(Driver { dev_id: id }).unwrap();
    }
}
