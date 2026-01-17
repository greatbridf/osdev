use core::ptr::NonNull;

use eonix_mm::address::PAddr;
use eonix_mm::paging::Folio as _;

use super::command::Command;
use super::{PRDTEntry, FISH2D};
use crate::kernel::mem::FolioOwned;

pub struct CommandTable {
    page: FolioOwned,
    cmd_fis: NonNull<FISH2D>,
    prdt: NonNull<[PRDTEntry; 248]>,
    prdt_entries: usize,
}

unsafe impl Send for CommandTable {}
unsafe impl Sync for CommandTable {}

impl CommandTable {
    pub fn new() -> Self {
        let page = FolioOwned::alloc();
        let base = page.get_ptr();

        unsafe {
            Self {
                page,
                cmd_fis: base.cast(),
                prdt: base.byte_add(0x80).cast(),
                prdt_entries: 0,
            }
        }
    }

    pub fn setup(&mut self, cmd: &impl Command) {
        unsafe {
            self.cmd_fis
                .as_mut()
                .setup(cmd.cmd(), cmd.lba(), cmd.count());
        }

        self.prdt_entries = cmd.pages().len();

        for (idx, page) in cmd.pages().iter().enumerate() {
            unsafe {
                self.prdt.as_mut()[idx].setup(page);
            }
        }
    }

    pub fn prdt_len(&self) -> usize {
        self.prdt_entries
    }

    pub fn base(&self) -> PAddr {
        self.page.start()
    }
}
