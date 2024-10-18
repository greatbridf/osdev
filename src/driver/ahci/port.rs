use bindings::EINVAL;

use crate::prelude::*;

use crate::kernel::block::{BlockDeviceRequest, BlockRequestQueue};
use crate::kernel::mem::paging::Page;

use crate::kernel::mem::phys::{NoCachePP, PhysPtr};

use super::command::{Command, IdentifyCommand, ReadLBACommand};
use super::{
    spinwait_clear, vread, vwrite, CommandHeader, PRDTEntry, ReceivedFis,
    ATA_DEV_BSY, ATA_DEV_DRQ, FISH2D, PORT_CMD_CR, PORT_CMD_FR, PORT_CMD_FRE,
    PORT_CMD_ST,
};

/// An `AdapterPort` is an HBA device in AHCI mode.
///
/// # Access
///
/// All reads and writes to this struct is volatile
///
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

pub struct AdapterPort<'lt> {
    nport: u32,
    data: &'lt mut AdapterPortData,
    page: Page,
    cmdheaders: &'lt mut [CommandHeader; 32],
    recv_fis: &'lt mut ReceivedFis,
}

impl<'lt> AdapterPort<'lt> {
    pub fn new(base: usize, nport: u32) -> Self {
        let page = Page::alloc_one();
        Self {
            nport,
            data: NoCachePP::new(base + 0x100 + 0x80 * nport as usize).as_mut(),
            cmdheaders: page.as_cached().as_mut(),
            recv_fis: page.as_cached().offset(0x400).as_mut(),
            page,
        }
    }
}

impl<'lt> AdapterPort<'lt> {
    pub fn status_ok(&self) -> bool {
        self.data.sata_status & 0xf == 0x3
    }

    fn stop_command(&mut self) -> KResult<()> {
        let cmd_status = vread(&self.data.command_status);
        vwrite(
            &mut self.data.command_status,
            cmd_status & !(PORT_CMD_ST | PORT_CMD_FRE),
        );

        spinwait_clear(&self.data.command_status, PORT_CMD_CR | PORT_CMD_FR)
    }

    fn start_command(&mut self) -> KResult<()> {
        spinwait_clear(&self.data.command_status, PORT_CMD_CR)?;

        let cmd_status = vread(&self.data.command_status);
        vwrite(
            &mut self.data.command_status,
            cmd_status | PORT_CMD_ST | PORT_CMD_FRE,
        );

        Ok(())
    }

    fn send_command(&mut self, cmd: &impl Command) -> KResult<()> {
        let pages = cmd.pages();

        // TODO: get an available command slot
        let cmdslot = 0;

        let cmdtable_page = Page::alloc_one();
        self.cmdheaders[cmdslot].clear();
        self.cmdheaders[cmdslot].setup(
            cmdtable_page.as_phys() as u64,
            pages.len() as u16,
            cmd.write(),
        );

        let command_fis: &mut FISH2D = cmdtable_page.as_cached().as_mut();
        command_fis.setup(cmd.cmd(), cmd.lba(), cmd.count());

        let prdt: &mut [PRDTEntry; 248] =
            cmdtable_page.as_cached().offset(0x80).as_mut();

        for (idx, page) in pages.iter().enumerate() {
            prdt[idx].setup(page);
        }

        // clear received fis?

        // wait until port is not busy
        spinwait_clear(&self.data.task_file_data, ATA_DEV_BSY | ATA_DEV_DRQ)?;

        vwrite(&mut self.data.command_issue, 1 << cmdslot);
        spinwait_clear(&self.data.command_issue, 1 << cmdslot)?;

        // TODO: check and wait interrupt

        Ok(())
    }

    fn identify(&mut self) -> KResult<()> {
        let cmd = IdentifyCommand::new();

        // TODO: check returned data
        self.send_command(&cmd)?;

        Ok(())
    }

    pub fn init(&mut self) -> KResult<()> {
        self.stop_command()?;

        // TODO: use interrupt
        // this is the PxIE register, setting bits here will make
        //      it generate corresponding interrupts in PxIS
        //
        // port->interrupt_enable = 1;

        vwrite(&mut self.data.command_list_base, self.page.as_phys() as u64);
        vwrite(&mut self.data.fis_base, self.page.as_phys() as u64 + 0x400);

        self.start_command()?;

        match self.identify() {
            Err(err) => {
                self.stop_command()?;
                return Err(err);
            }
            Ok(_) => Ok(()),
        }
    }
}

impl<'lt> BlockRequestQueue for AdapterPort<'lt> {
    fn max_request_pages(&self) -> u64 {
        1024
    }

    fn submit(&mut self, req: BlockDeviceRequest) -> KResult<()> {
        // TODO: check disk size limit using newtype
        if req.count > 65535 {
            return Err(EINVAL);
        }

        let command =
            ReadLBACommand::new(req.buffer, req.sector, req.count as u16)?;

        self.send_command(&command)
    }
}
