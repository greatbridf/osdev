use super::defs;
use super::error::E1000eError;
use super::rx_desc::RxDescriptorTable;
use super::tx_desc::TxDescriptorTable;
use crate::net::netdev::{NetDevice, PhyDevice};
use crate::net::{LinkSpeed, LinkState, LinkStatus, NetBuffer, NetError};
use crate::prelude::*;
use crate::sync::fence::memory_barrier;
use crate::{kernel::interrupt::register_irq_handler, net::Mac};
use alloc::sync::Arc;
use core::ops::DerefMut;
use core::ptr::NonNull;
use eonix_hal::mm::ArchPhysAccess;
use eonix_mm::address::{Addr, PAddr, PRange, PhysAccess};
use eonix_sync::{SpinIrq, WaitList};

pub struct Registers(NonNull<()>);

unsafe impl Send for Registers {}
unsafe impl Sync for Registers {}

struct Stats {
    packet_rx: usize,
    packet_tx: usize,
    byte_rx: usize,
    byte_tx: usize,
    /// Multicast packets received
    mprx: usize,
}

pub struct E1000eInner {
    up: bool,
    speed_megs: Option<u32>,
    mac: Mac,

    stats: Stats,

    regs: Registers,
    rx_desc_table: RxDescriptorTable,
    tx_desc_table: TxDescriptorTable,
}

pub struct E1000eDev {
    irq_no: usize,

    inner: Spin<E1000eInner>,
    wait_list: WaitList,
}

#[derive(Clone)]
pub struct E1000eHandle {
    pub dev: Arc<E1000eDev>,
}

fn test(val: u32, bit: u32) -> bool {
    (val & bit) == bit
}

impl Registers {
    fn new(base: PAddr) -> Self {
        Self(unsafe { ArchPhysAccess::as_ptr(base) })
    }

    pub fn read(&self, offset: u32) -> u32 {
        memory_barrier();
        let retval = unsafe {
            // SAFETY: The offset is within the bounds of the device's memory-mapped registers.
            self.0.byte_offset(offset as isize).cast().read_volatile()
        };
        memory_barrier();
        retval
    }

    pub fn write(&self, offset: u32, value: u32) {
        memory_barrier();
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
        memory_barrier();
        let retval = unsafe {
            // SAFETY: The offset is within the bounds of the device's memory-mapped registers.
            self.0.byte_offset(offset as isize).cast().read_volatile()
        };
        memory_barrier();
        retval
    }
}

impl Stats {
    const fn new() -> Self {
        Self {
            packet_rx: 0,
            packet_tx: 0,
            byte_rx: 0,
            byte_tx: 0,
            mprx: 0,
        }
    }

    fn update_tx(&mut self, regs: &Registers) {
        self.packet_tx += regs.read(defs::REG_GPTC) as usize;

        self.byte_tx +=
            regs.read(defs::REG_GOTCL) as usize + (regs.read(defs::REG_GOTCH) as usize) << 32;
    }

    fn update_rx(&mut self, regs: &Registers) {
        self.packet_rx += regs.read(defs::REG_GPRC) as usize;
        self.mprx += regs.read(defs::REG_MPRC) as usize;

        self.byte_rx +=
            regs.read(defs::REG_GORCL) as usize + (regs.read(defs::REG_GORCH) as usize) << 32;
    }
}

impl PhyDevice for E1000eHandle {
    type Device = E1000eInner;

    fn device(&self) -> impl DerefMut<Target = Self::Device> {
        self.dev.inner.lock_irq()
    }

    fn state(&self) -> LinkState {
        let inner = self.dev.inner.lock_irq();
        let status = if inner.up { LinkStatus::Up } else { LinkStatus::Down };
        let speed = match inner.speed_megs {
            Some(speed) => LinkSpeed::SpeedMegs(speed as usize),
            None => LinkSpeed::SpeedUnknown,
        };

        LinkState {
            status,
            speed,
            mac: inner.mac,
        }
    }

    fn up(&self) -> Result<(), NetError> {
        self.dev.up()
    }

    fn down(&self) -> Result<(), NetError> {
        unimplemented!("E1000e: down() is not implemented yet");
    }
}

impl E1000eInner {
    fn send(&mut self, buf: &dyn NetBuffer) -> Result<(), E1000eError> {
        self.tx_desc_table.send(&self.regs, buf.as_phys())?;

        Ok(())
    }

    fn up(&mut self) -> Result<(), E1000eError> {
        let ctrl = self.regs.read(defs::REG_CTRL);
        let status = self.regs.read(defs::REG_STAT);

        // check link up
        if !test(ctrl, defs::CTRL_SLU) || !test(status, defs::STAT_LU) {
            return Err(E1000eError::DeviceNotReady);
        }

        // auto negotiation of speed
        match status & defs::STAT_SPEED_MASK {
            defs::STAT_SPEED_10M => self.speed_megs = Some(10),
            defs::STAT_SPEED_100M => self.speed_megs = Some(100),
            defs::STAT_SPEED_1000M => self.speed_megs = Some(1000),
            _ => return Err(E1000eError::UnsupportedSpeed),
        }

        // clear multicast table
        for i in (0..128).step_by(4) {
            self.regs.write(defs::REG_MTA + i, 0);
        }

        self.clear_stats()?;

        // enable interrupts
        self.regs
            .write(defs::REG_IMS, defs::ICR_NORMAL | defs::ICR_UP);

        // read to clear any pending interrupts
        self.regs.read(defs::REG_ICR);

        self.setup_rx()?;
        self.setup_tx()?;

        self.up = true;

        Ok(())
    }

    fn on_rx_done(&mut self) {
        self.stats.update_rx(&self.regs);

        // TODO: Call the wakers if any.
    }

    fn on_tx_done(&mut self) {
        self.stats.update_tx(&self.regs);

        // TODO: Wake up the TX wakers if any.
    }

    fn fire(&mut self) -> Result<(), u32> {
        let cause = self.regs.read(defs::REG_ICR);

        if !test(cause, defs::ICR_INT) {
            return Ok(());
        }

        if test(cause, defs::ICR_TXDW) {
            println_trace!("trace_net", "E1000e: TX done");
            self.on_tx_done();
        }

        if test(cause, defs::ICR_RXT0) {
            println_trace!("trace_net", "E1000e: RX done");
            self.on_rx_done();
        }

        Ok(())
    }

    fn setup_rx(&mut self) -> Result<(), E1000eError> {
        let base = self.rx_desc_table.base();

        self.regs.write(defs::REG_RDBAL, base.addr() as u32);
        self.regs.write(defs::REG_RDBAH, (base.addr() >> 32) as u32);

        self.regs
            .write(defs::REG_RDLEN, self.rx_desc_table.byte_len());

        self.regs.write(defs::REG_RDH, 0);
        self.regs.write(defs::REG_RDT, self.rx_desc_table.tail());

        self.regs.write(defs::REG_RDTR, 8); // 8us delay

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

    fn setup_tx(&mut self) -> Result<(), E1000eError> {
        let base = self.tx_desc_table.base();

        self.regs.write(defs::REG_TDBAL, base.addr() as u32);
        self.regs.write(defs::REG_TDBAH, (base.addr() >> 32) as u32);

        self.regs
            .write(defs::REG_TDLEN, self.tx_desc_table.byte_len());

        self.regs.write(defs::REG_TDH, 0);
        self.regs.write(defs::REG_TDT, self.tx_desc_table.tail());

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

    fn reset(&self) -> Result<(), E1000eError> {
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

    fn clear_stats(&self) -> Result<(), E1000eError> {
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

    fn new(base: PAddr) -> Result<Self, E1000eError> {
        let mut dev = Self {
            up: false,
            speed_megs: None,
            mac: Mac::zeros(),
            stats: Stats::new(),
            regs: Registers::new(base),
            rx_desc_table: RxDescriptorTable::new(32)?,
            tx_desc_table: TxDescriptorTable::new(32)?,
        };

        dev.reset()?;

        let mac: [u8; 6] = dev.regs.read_as(0x5400);
        dev.mac = Mac::new(mac);

        Ok(dev)
    }
}

impl Drop for E1000eInner {
    fn drop(&mut self) {
        assert!(!self.up, "E1000e device is still up when being dropped");
    }
}

impl E1000eDev {
    pub fn create(irq_no: usize, base: PAddr) -> Result<NetDevice<E1000eHandle>, E1000eError> {
        let dev_handle = E1000eHandle {
            dev: Arc::new(Self {
                irq_no,
                inner: Spin::new(E1000eInner::new(base)?),
                wait_list: WaitList::new(),
            }),
        };

        Ok(NetDevice::new(dev_handle))
    }

    fn up(self: &Arc<Self>) -> Result<(), NetError> {
        self.inner.lock_irq().up()?;

        let dev = self.clone();
        register_irq_handler(self.irq_no as i32, move || {
            dev.inner
                .lock()
                .fire()
                .expect("e1000e: failed to handle interrupt");
        })
        .map_err(|code| NetError::SystemError(code, "Cannot register IRQ handler"))?;

        Ok(())
    }
}

pub struct RxToken<'a, 'r> {
    table: &'a mut RxDescriptorTable,
    regs: &'r Registers,
}

pub struct TxToken<'a, 'r> {
    table: &'a mut TxDescriptorTable,
    regs: &'r Registers,
}

impl smoltcp::phy::RxToken for RxToken<'_, '_> {
    fn consume<R, F: FnOnce(&[u8]) -> R>(self, f: F) -> R {
        let buf = self
            .table
            .received(&self.regs)
            .next()
            .expect("There should be a packet available");

        f(buf.as_bytes())
    }
}

impl smoltcp::phy::TxToken for TxToken<'_, '_> {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
        let mut buffer = [0; 1536];
        let result = f(&mut buffer[..len]);

        let buffer_addr = unsafe {
            // SAFETY: `buffer` is in the kernel space and a mutable reference can't be null.
            ArchPhysAccess::from_ptr(NonNull::new_unchecked(&mut buffer))
        };

        self.table
            .send(self.regs, PRange::from(buffer_addr).grow(len))
            .expect("There should be space in TX buffer");

        result
    }
}

impl smoltcp::phy::Device for E1000eInner {
    type RxToken<'a>
        = RxToken<'a, 'a>
    where
        Self: 'a;

    type TxToken<'a>
        = TxToken<'a, 'a>
    where
        Self: 'a;

    fn receive(
        &mut self,
        _timestamp: smoltcp::time::Instant,
    ) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        if !self.up {
            return None; // Device is not up
        }

        if !self.rx_desc_table.has_data(&self.regs) {
            return None; // No new packets
        }

        if !self.tx_desc_table.can_send(&self.regs) {
            return None; // No space in TX buffer
        }

        Some((
            RxToken {
                table: &mut self.rx_desc_table,
                regs: &self.regs,
            },
            TxToken {
                table: &mut self.tx_desc_table,
                regs: &self.regs,
            },
        ))
    }

    fn transmit(&mut self, _timestamp: smoltcp::time::Instant) -> Option<Self::TxToken<'_>> {
        if !self.up {
            return None; // Device is not up
        }

        if !self.tx_desc_table.can_send(&self.regs) {
            return None; // No space in TX buffer
        }

        Some(TxToken {
            table: &mut self.tx_desc_table,
            regs: &self.regs,
        })
    }

    fn capabilities(&self) -> smoltcp::phy::DeviceCapabilities {
        let mut caps = smoltcp::phy::DeviceCapabilities::default();

        caps.medium = smoltcp::phy::Medium::Ethernet;
        caps.max_transmission_unit = 1536;
        caps.max_burst_size = Some(32);

        caps
    }
}
