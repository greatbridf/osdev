use super::{command::Command, PRDTEntry, FISH2D};
use crate::kernel::mem::{AsMemoryBlock as _, Page};
use eonix_mm::address::PAddr;

pub struct CommandTable<'a> {
    page: Page,
    command_fis: &'a mut FISH2D,

    prdt: &'a mut [PRDTEntry; 248],
    prdt_entries: Option<u16>,
}

impl CommandTable<'_> {
    pub fn new() -> Self {
        let page = Page::alloc();
        let memory = page.as_memblk();

        let (lhs, prdt) = memory.split_at(0x80);

        let (command_fis, _) = lhs.split_at(size_of::<FISH2D>());
        let command_fis = unsafe { command_fis.as_ptr().as_mut() };
        let prdt = unsafe { prdt.as_ptr().as_mut() };

        Self {
            page,
            command_fis,
            prdt,
            prdt_entries: None,
        }
    }

    pub fn setup(&mut self, cmd: &impl Command) {
        self.command_fis.setup(cmd.cmd(), cmd.lba(), cmd.count());
        self.prdt_entries = Some(cmd.pages().len() as u16);

        for (idx, page) in cmd.pages().iter().enumerate() {
            self.prdt[idx].setup(page);
        }
    }

    pub fn prdt_len(&self) -> u16 {
        self.prdt_entries.unwrap()
    }

    pub fn base(&self) -> PAddr {
        self.page.start()
    }
}
