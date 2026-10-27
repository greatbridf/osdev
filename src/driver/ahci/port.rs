use alloc::collections::vec_deque::VecDeque;
use core::task::{Poll, Waker};

use async_trait::async_trait;
use eonix_mm::address::{Addr as _, PAddr};
use eonix_sync::SpinIrq as _;

use super::command::{Command, IdentifyCommand, ReadLBACommand, WriteLBACommand};
use super::slot::CommandList;
use super::stats::AdapterPortStats;
use super::{Register, PORT_CMD_CR, PORT_CMD_FR, PORT_CMD_FRE, PORT_CMD_ST, PORT_IE_DEFAULT};
use crate::driver::ahci::command_table::CommandTable;
use crate::kernel::block::{BlockDeviceRequest, BlockRequestQueue};
use crate::kernel::constants::{EINVAL, EIO};
use crate::prelude::*;

/// An `AdapterPort` is an HBA device in AHCI mode.
///
/// # Access
///
/// All reads and writes to this struct is volatile
///
#[allow(dead_code)]
#[repr(C)]
pub struct AdapterPortData {
    pub command_list_base: u64,
    pub fis_base: u64,

    pub interrupt_status: u32,
    pub interrupt_enable: u32,

    pub command_status: u32,

    _reserved2: u32,

    pub task_file_data: u32,
    pub signature: u32,

    pub sata_status: u32,
    pub sata_control: u32,
    pub sata_error: u32,
    pub sata_active: u32,

    pub command_issue: u32,
    pub sata_notification: u32,

    pub fis_based_switch_control: u32,

    _reserved1: [u32; 11],
    vendor: [u32; 4],
}

struct FreeList {
    free: VecDeque<u32>,
    working: VecDeque<u32>,

    wakers: VecDeque<Waker>,
}

impl FreeList {
    fn new() -> Self {
        Self {
            free: (0..32).collect(),
            working: VecDeque::new(),
            wakers: VecDeque::new(),
        }
    }
}

pub struct AdapterPort {
    pub nport: u32,
    regs_base: PAddr,

    cmdlist: CommandList,
    free_list: Spin<FreeList>,

    stats: AdapterPortStats,
}

impl AdapterPort {
    pub fn new(base: PAddr, nport: u32) -> Self {
        Self {
            nport,
            regs_base: base + 0x100 + 0x80 * nport as usize,
            cmdlist: CommandList::new(),
            free_list: Spin::new(FreeList::new()),
            stats: AdapterPortStats::new(),
        }
    }

    fn command_list_base(&self) -> Register<u64> {
        Register::new(self.regs_base + 0x00)
    }

    fn fis_base(&self) -> Register<u64> {
        Register::new(self.regs_base + 0x08)
    }

    fn sata_status(&self) -> Register<u32> {
        Register::new(self.regs_base + 0x28)
    }

    fn command_status(&self) -> Register<u32> {
        Register::new(self.regs_base + 0x18)
    }

    fn command_issue(&self) -> Register<u32> {
        Register::new(self.regs_base + 0x38)
    }

    pub fn interrupt_status(&self) -> Register<u32> {
        Register::new(self.regs_base + 0x10)
    }

    fn interrupt_enable(&self) -> Register<u32> {
        Register::new(self.regs_base + 0x14)
    }

    pub fn status_ok(&self) -> bool {
        self.sata_status().read_once() & 0xf == 0x3
    }

    async fn get_free_slot(&self) -> u32 {
        core::future::poll_fn(|ctx| {
            let mut free_list = self.free_list.lock_irq();
            if let Some(slot) = free_list.free.pop_front() {
                return Poll::Ready(slot);
            }

            free_list.wakers.push_back(ctx.waker().clone());
            Poll::Pending
        })
        .await
    }

    fn save_working(&self, slot: u32) {
        self.free_list.lock_irq().working.push_back(slot);
    }

    fn release_free_slot(&self, slot: u32) {
        let mut free_list = self.free_list.lock_irq();

        free_list.free.push_back(slot);
        free_list.wakers.drain(..).for_each(|waker| waker.wake());
    }

    pub fn handle_interrupt(&self) {
        let ci = self.command_issue().read_once();

        // no need to use `lock_irq()` inside interrupt handler
        let mut free_list = self.free_list.lock();

        free_list.working.retain(|&n| {
            if ci & (1 << n) != 0 {
                return true;
            }

            self.cmdlist.get(n as usize).handle_irq();
            self.stats.inc_int_fired();

            false
        });
    }

    fn stop_command(&self) -> KResult<()> {
        let status_reg = self.command_status();
        let status = status_reg.read();
        status_reg.write_once(status & !(PORT_CMD_ST | PORT_CMD_FRE));
        status_reg.spinwait_clear(PORT_CMD_CR | PORT_CMD_FR)
    }

    fn start_command(&self) -> KResult<()> {
        let status_reg = self.command_status();
        status_reg.spinwait_clear(PORT_CMD_CR)?;

        let status = status_reg.read();
        status_reg.write_once(status | PORT_CMD_ST | PORT_CMD_FRE);

        Ok(())
    }

    async fn send_command(&self, cmd: &impl Command) -> KResult<()> {
        let mut cmdtable = CommandTable::new();
        cmdtable.setup(cmd);

        let slot_index = self.get_free_slot().await;
        let slot = self.cmdlist.get(slot_index as usize);

        slot.prepare_command(&cmdtable, cmd.write());
        self.save_working(slot_index);

        let cmdissue_reg = self.command_issue();

        // should we clear received fis here?
        debug_assert!(cmdissue_reg.read_once() & (1 << slot_index) == 0);
        cmdissue_reg.write_once(1 << slot_index);

        self.stats.inc_cmd_sent();

        slot.wait_finish().await.inspect_err(|_| {
            self.stats.inc_cmd_error();
        })?;

        self.release_free_slot(slot_index);
        Ok(())
    }

    async fn identify(&self) -> KResult<()> {
        let cmd = IdentifyCommand::new();

        // TODO: check returned data
        self.send_command(&cmd).await?;

        Ok(())
    }

    pub async fn init(&self) -> KResult<()> {
        self.stop_command()?;

        self.command_list_base()
            .write(self.cmdlist.cmdlist_base().addr() as u64);
        self.fis_base()
            .write(self.cmdlist.recv_fis_base().addr() as u64);

        self.interrupt_enable().write_once(PORT_IE_DEFAULT);

        self.start_command()?;

        match self.identify().await {
            Err(err) => {
                self.stop_command()?;
                Err(err)
            }
            Ok(_) => Ok(()),
        }
    }

    pub fn print_stats(&self, writer: &mut impl Write) -> KResult<()> {
        writeln!(writer, "cmd_sent: {}", self.stats.get_cmd_sent()).map_err(|_| EIO)?;
        writeln!(writer, "cmd_error: {}", self.stats.get_cmd_error()).map_err(|_| EIO)?;
        writeln!(writer, "int_fired: {}", self.stats.get_int_fired()).map_err(|_| EIO)?;

        Ok(())
    }
}

#[async_trait]
impl BlockRequestQueue for AdapterPort {
    fn max_request_pages(&self) -> u64 {
        1024
    }

    async fn submit<'a>(&'a self, req: BlockDeviceRequest<'a>) -> KResult<()> {
        match req {
            BlockDeviceRequest::Read {
                sector,
                count,
                buffer,
            } => {
                if count > 65535 {
                    return Err(EINVAL);
                }

                let command = ReadLBACommand::new(buffer, sector, count as u16)?;

                self.send_command(&command).await
            }
            BlockDeviceRequest::Write {
                sector,
                count,
                buffer,
            } => {
                if count > 65535 {
                    return Err(EINVAL);
                }

                let command = WriteLBACommand::new(buffer, sector, count as u16)?;

                self.send_command(&command).await
            }
        }
    }
}
