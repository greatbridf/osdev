use crate::kernel::constants::EINVAL;
use crate::kernel::mem::paging::Page;
use crate::prelude::*;

pub trait Command {
    fn pages(&self) -> &[Page];
    fn lba(&self) -> u64;

    // in sectors
    fn count(&self) -> u16;

    fn cmd(&self) -> u8;
    fn write(&self) -> bool;
}

pub struct IdentifyCommand {
    page: Page,
}

impl IdentifyCommand {
    pub fn new() -> Self {
        Self {
            page: Page::alloc(),
        }
    }
}

impl Command for IdentifyCommand {
    fn pages(&self) -> &[Page] {
        core::slice::from_ref(&self.page)
    }

    fn lba(&self) -> u64 {
        0
    }

    fn count(&self) -> u16 {
        1
    }

    fn cmd(&self) -> u8 {
        0xEC
    }

    fn write(&self) -> bool {
        false
    }
}

pub struct ReadLBACommand<'lt> {
    pages: &'lt [Page],
    lba: u64,
    count: u16,
}

impl<'lt> ReadLBACommand<'lt> {
    pub fn new(pages: &'lt [Page], lba: u64, count: u16) -> KResult<Self> {
        if pages.len() > 248 {
            return Err(EINVAL);
        }

        let buffer_tot_len = pages.iter().fold(0, |acc, page| acc + page.len());
        if buffer_tot_len < count as usize * 512 {
            return Err(EINVAL);
        }

        Ok(Self { pages, lba, count })
    }
}

impl Command for ReadLBACommand<'_> {
    fn pages(&self) -> &[Page] {
        self.pages
    }

    fn lba(&self) -> u64 {
        self.lba
    }

    fn count(&self) -> u16 {
        self.count
    }

    fn cmd(&self) -> u8 {
        0xC8
    }

    fn write(&self) -> bool {
        false
    }
}

pub struct WriteLBACommand<'a> {
    pages: &'a [Page],
    lba: u64,
    count: u16,
}

impl<'a> WriteLBACommand<'a> {
    pub fn new(pages: &'a [Page], lba: u64, count: u16) -> KResult<Self> {
        if pages.len() > 248 {
            return Err(EINVAL);
        }

        let buffer_tot_len = pages.iter().fold(0, |acc, page| acc + page.len());
        if buffer_tot_len < count as usize * 512 {
            return Err(EINVAL);
        }

        Ok(Self { pages, lba, count })
    }
}

impl Command for WriteLBACommand<'_> {
    fn pages(&self) -> &[Page] {
        self.pages
    }

    fn lba(&self) -> u64 {
        self.lba
    }

    fn count(&self) -> u16 {
        self.count
    }

    fn cmd(&self) -> u8 {
        0xCA
    }

    fn write(&self) -> bool {
        true
    }
}
