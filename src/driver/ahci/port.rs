use super::command::{Command, IdentifyCommand, ReadLBACommand, WriteLBACommand};
use super::slot::CommandSlot;
use super::stats::AdapterPortStats;
use super::{
    CommandHeader, Register, PORT_CMD_CR, PORT_CMD_FR, PORT_CMD_FRE, PORT_CMD_ST, PORT_IE_DEFAULT,
};
use crate::driver::ahci::command_table::CommandTable;
use crate::kernel::block::{BlockDeviceRequest, BlockRequestQueue};
use crate::kernel::constants::{EINVAL, EIO};
use crate::kernel::mem::paging::Page;
use crate::kernel::mem::AsMemoryBlock as _;
use crate::prelude::*;
use alloc::collections::vec_deque::VecDeque;
use core::pin::pin;
use eonix_mm::address::{Addr as _, PAddr};
use eonix_runtime::task::Task;
use eonix_sync::{SpinIrq as _, WaitList};

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
}

impl FreeList {
    fn new() -> Self {
        Self {
            free: (0..32).collect(),
            working: VecDeque::new(),
        }
    }
}

pub struct AdapterPort<'a> {
    pub nport: u32,
    regs_base: PAddr,

    slots: [CommandSlot<'a>; 32],
    free_list: Spin<FreeList>,
    free_list_wait: WaitList,

    /// Holds the command list.
    /// **DO NOT USE IT DIRECTLY**
    _page: Page,

    cmdlist_base: PAddr,
    fis_base: PAddr,

    stats: AdapterPortStats,
}

impl<'a> AdapterPort<'a> {
    pub fn new(base: PAddr, nport: u32) -> Self {
        let page = Page::alloc();
        let cmdlist_base = page.start();
        let cmdlist_size = 32 * size_of::<CommandHeader>();
        let fis_base = cmdlist_base + cmdlist_size;

        let (mut cmdheaders, _) = page.as_memblk().split_at(cmdlist_size);
        let slots = core::array::from_fn(move |_| {
            let (cmdheader, next) = cmdheaders.split_at(size_of::<CommandHeader>());
            cmdheaders = next;
            CommandSlot::new(unsafe { cmdheader.as_ptr().as_mut() })
        });

        Self {
            nport,
            regs_base: base + 0x100 + 0x80 * nport as usize,
            slots,
            free_list: Spin::new(FreeList::new()),
            free_list_wait: WaitList::new(),
            _page: page,
            stats: AdapterPortStats::new(),
            cmdlist_base,
            fis_base,
        }
    }
}

impl AdapterPort<'_> {
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

    fn get_free_slot(&self) -> u32 {
        loop {
            let mut free_list = self.free_list.lock_irq();
            let free_slot = free_list.free.pop_front();
            if let Some(slot) = free_slot {
                return slot;
            }
            let mut wait = pin!(self.free_list_wait.prepare_to_wait());
            wait.as_mut().add_to_wait_list();
            drop(free_list);

            Task::block_on(wait);
        }
    }

    fn save_working(&self, slot: u32) {
        self.free_list.lock_irq().working.push_back(slot);
    }

    fn release_free_slot(&self, slot: u32) {
        self.free_list.lock_irq().free.push_back(slot);
        self.free_list_wait.notify_one();
    }

    pub fn handle_interrupt(&self) {
        let ci = self.command_issue().read_once();

        // no need to use `lock_irq()` inside interrupt handler
        let mut free_list = self.free_list.lock();

        free_list.working.retain(|&n| {
            if ci & (1 << n) != 0 {
                return true;
            }

            self.slots[n as usize].handle_irq();
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

    fn send_command(&self, cmd: &impl Command) -> KResult<()> {
        let mut cmdtable = CommandTable::new();
        cmdtable.setup(cmd);

        let slot_index = self.get_free_slot();
        let slot = &self.slots[slot_index as usize];

        slot.prepare_command(&cmdtable, cmd.write());
        self.save_working(slot_index);

        let cmdissue_reg = self.command_issue();

        // should we clear received fis here?
        debug_assert!(cmdissue_reg.read_once() & (1 << slot_index) == 0);
        cmdissue_reg.write_once(1 << slot_index);

        self.stats.inc_cmd_sent();

        if let Err(_) = Task::block_on(slot.wait_finish()) {
            self.stats.inc_cmd_error();
            return Err(EIO);
        };

        self.release_free_slot(slot_index);
        Ok(())
    }

    fn identify(&self) -> KResult<()> {
        let cmd = IdentifyCommand::new();

        // TODO: check returned data
        self.send_command(&cmd)?;

        Ok(())
    }

    pub fn init(&self) -> KResult<()> {
        self.stop_command()?;

        self.command_list_base()
            .write(self.cmdlist_base.addr() as u64);
        self.fis_base().write(self.fis_base.addr() as u64);

        self.interrupt_enable().write_once(PORT_IE_DEFAULT);

        self.start_command()?;

        match self.identify() {
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

impl BlockRequestQueue for AdapterPort<'_> {
    fn max_request_pages(&self) -> u64 {
        1024
    }

    fn submit(&self, req: BlockDeviceRequest) -> KResult<()> {
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

                self.send_command(&command)
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

                self.send_command(&command)
            }
        }
    }
}
