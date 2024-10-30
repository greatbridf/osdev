use alloc::collections::vec_deque::VecDeque;
use bindings::{EINVAL, EIO};

use crate::prelude::*;

use crate::kernel::block::{BlockDeviceRequest, BlockRequestQueue};
use crate::kernel::mem::paging::Page;

use crate::kernel::mem::phys::{NoCachePP, PhysPtr};
use crate::sync::condvar::CondVar;

use super::command::{Command, IdentifyCommand, ReadLBACommand};
use super::{
    vread, vwrite, CommandHeader, PRDTEntry, FISH2D, PORT_CMD_CR, PORT_CMD_FR,
    PORT_CMD_FRE, PORT_CMD_ST, PORT_IE_DEFAULT,
};

fn spinwait_clear(refval: *const u32, mask: u32) -> KResult<()> {
    const SPINWAIT_MAX: usize = 1000;

    let mut spins = 0;
    while vread(refval) & mask != 0 {
        if spins == SPINWAIT_MAX {
            return Err(EIO);
        }

        spins += 1;
    }

    Ok(())
}

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

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum SlotState {
    Idle,
    Working,
    Finished,
    Error,
}

struct CommandSlotInner {
    state: SlotState,
    /// # Usage
    /// `cmdheader` might be used in irq handler. So in order to wait for
    /// commands to finish, we should use `lock_irq` on `cmdheader`
    cmdheader: *mut CommandHeader,
}

/// # Safety
/// This is safe because the `cmdheader` is not shared between threads
unsafe impl Send for CommandSlotInner {}

impl CommandSlotInner {
    pub fn setup(&mut self, cmdtable_base: u64, prdtlen: u16, write: bool) {
        let cmdheader = unsafe { self.cmdheader.as_mut().unwrap() };
        cmdheader.first = 0x05; // FIS type

        if write {
            cmdheader.first |= 0x40;
        }

        cmdheader.second = 0x00;

        cmdheader.prdt_length = prdtlen;
        cmdheader.bytes_transferred = 0;
        cmdheader.command_table_base = cmdtable_base;

        cmdheader._reserved = [0; 4];
    }
}

struct CommandSlot {
    inner: Spin<CommandSlotInner>,
    cv: CondVar,
}

impl CommandSlot {
    fn new(cmdheader: *mut CommandHeader) -> Self {
        Self {
            inner: Spin::new(CommandSlotInner {
                state: SlotState::Idle,
                cmdheader,
            }),
            cv: CondVar::new(),
        }
    }
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

pub struct AdapterPort {
    nport: u32,
    regs: *mut (),
    page: Page,
    slots: [CommandSlot; 32],
    free_list: Spin<FreeList>,
    free_list_cv: CondVar,
}

/// # Safety
/// This is safe because the `AdapterPort` can be accessed by only one thread at the same time
unsafe impl Send for AdapterPort {}
unsafe impl Sync for AdapterPort {}

impl AdapterPort {
    pub fn new(base: usize, nport: u32) -> Self {
        let page = Page::alloc_one();
        let cmdheaders_start = page.as_cached().as_ptr::<CommandHeader>();

        Self {
            nport,
            regs: NoCachePP::new(base + 0x100 + 0x80 * nport as usize).as_ptr(),
            slots: core::array::from_fn(|index| {
                CommandSlot::new(unsafe {
                    cmdheaders_start.offset(index as isize)
                })
            }),
            free_list: Spin::new(FreeList::new()),
            free_list_cv: CondVar::new(),
            page,
        }
    }
}

impl AdapterPort {
    fn command_list_base(&self) -> *mut u64 {
        unsafe { self.regs.byte_offset(0x00).cast() }
    }

    fn fis_base(&self) -> *mut u64 {
        unsafe { self.regs.byte_offset(0x08).cast() }
    }

    fn sata_status(&self) -> *mut u32 {
        unsafe { self.regs.byte_offset(0x28).cast() }
    }

    fn command_status(&self) -> *mut u32 {
        unsafe { self.regs.byte_offset(0x18).cast() }
    }

    fn command_issue(&self) -> *mut u32 {
        unsafe { self.regs.byte_offset(0x38).cast() }
    }

    pub fn interrupt_status(&self) -> *mut u32 {
        unsafe { self.regs.byte_offset(0x10).cast() }
    }

    pub fn interrupt_enable(&self) -> *mut u32 {
        unsafe { self.regs.byte_offset(0x14).cast() }
    }

    pub fn status_ok(&self) -> bool {
        vread(self.sata_status()) & 0xf == 0x3
    }

    fn get_free_slot(&self) -> usize {
        let mut free_list = self.free_list.lock_irq();

        loop {
            match free_list.free.pop_front() {
                Some(slot) => {
                    free_list.working.push_back(slot);
                    break slot as usize;
                }
                None => {
                    self.free_list_cv.wait(&mut free_list, false);
                }
            }
        }
    }

    fn release_free_slot(&self, slot: u32) {
        self.free_list.lock().free.push_back(slot);
        self.free_list_cv.notify_one();
    }

    pub fn handle_interrupt(&self) {
        let ci = vread(self.command_issue());

        // no need to use `lock_irq()` inside interrupt handler
        let mut free_list = self.free_list.lock();

        free_list.working.retain(|&n| {
            if ci & (1 << n) != 0 {
                return true;
            }

            let slot = &self.slots[n as usize];

            println_debug!("slot{n} finished");

            // TODO: check error
            slot.inner.lock().state = SlotState::Finished;
            slot.cv.notify_all();

            false
        });
    }

    fn stop_command(&self) -> KResult<()> {
        vwrite(
            self.command_status(),
            vread(self.command_status()) & !(PORT_CMD_ST | PORT_CMD_FRE),
        );

        spinwait_clear(self.command_status(), PORT_CMD_CR | PORT_CMD_FR)
    }

    fn start_command(&self) -> KResult<()> {
        spinwait_clear(self.command_status(), PORT_CMD_CR)?;

        let cmd_status = vread(self.command_status());
        vwrite(
            self.command_status(),
            cmd_status | PORT_CMD_ST | PORT_CMD_FRE,
        );

        Ok(())
    }

    /// # Might Sleep
    /// This function **might sleep**, so call it in a preemptible context
    fn send_command(&self, cmd: &impl Command) -> KResult<()> {
        might_sleep!();

        let pages = cmd.pages();
        let cmdtable_page = Page::alloc_one();

        let command_fis: &mut FISH2D = cmdtable_page.as_cached().as_mut();
        command_fis.setup(cmd.cmd(), cmd.lba(), cmd.count());

        let prdt: &mut [PRDTEntry; 248] =
            cmdtable_page.as_cached().offset(0x80).as_mut();

        for (idx, page) in pages.iter().enumerate() {
            prdt[idx].setup(page);
        }

        let slot_index = self.get_free_slot();
        let slot_object = &self.slots[slot_index];
        let mut slot = slot_object.inner.lock_irq();

        slot.setup(
            cmdtable_page.as_phys() as u64,
            pages.len() as u16,
            cmd.write(),
        );
        slot.state = SlotState::Working;

        // should we clear received fis here?
        debug_assert!(vread(self.command_issue()) & (1 << slot_index) == 0);

        println_debug!("slot{slot_index} working");
        vwrite(self.command_issue(), 1 << slot_index);

        while slot.state == SlotState::Working {
            slot_object.cv.wait(&mut slot, false);
        }

        let state = slot.state;
        slot.state = SlotState::Idle;

        self.release_free_slot(slot_index as u32);

        drop(slot);
        println_debug!("slot{slot_index} released");

        match state {
            SlotState::Finished => Ok(()),
            SlotState::Error => Err(EIO),
            _ => panic!("Invalid slot state"),
        }
    }

    fn identify(&self) -> KResult<()> {
        let cmd = IdentifyCommand::new();

        // TODO: check returned data
        self.send_command(&cmd)?;

        Ok(())
    }

    pub fn init(&self) -> KResult<()> {
        self.stop_command()?;

        vwrite(self.interrupt_enable(), PORT_IE_DEFAULT);

        vwrite(self.command_list_base(), self.page.as_phys() as u64);
        vwrite(self.fis_base(), self.page.as_phys() as u64 + 0x400);

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

impl BlockRequestQueue for AdapterPort {
    fn max_request_pages(&self) -> u64 {
        1024
    }

    fn submit(&self, req: BlockDeviceRequest) -> KResult<()> {
        // TODO: check disk size limit using newtype
        if req.count > 65535 {
            return Err(EINVAL);
        }

        let command =
            ReadLBACommand::new(req.buffer, req.sector, req.count as u16)?;

        self.send_command(&command)
    }
}
