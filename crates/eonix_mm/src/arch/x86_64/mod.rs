use crate::address::{Addr as _, PAddr};
use crate::page_table::{PageAttribute, TableAttribute, PTE};
use crate::paging::PFN;

const PA_P: u64 = 0x001;
const PA_RW: u64 = 0x002;
const PA_US: u64 = 0x004;
#[allow(dead_code)]
const PA_PWT: u64 = 0x008;
#[allow(dead_code)]
const PA_PCD: u64 = 0x010;
const PA_A: u64 = 0x020;
const PA_D: u64 = 0x040;
const PA_PS: u64 = 0x080;
const PA_G: u64 = 0x100;
const PA_COW: u64 = 0x200;
const PA_MMAP: u64 = 0x400;
const PA_ANON: u64 = 0x800;
const PA_NXE: u64 = 0x8000_0000_0000_0000;
const PA_MASK: u64 = 0xfff0_0000_0000_0fff;

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct RawPTE(u64);

impl PTE for RawPTE {
    fn get_pfn(&self) -> PFN {
        PFN::from(PAddr::from((self.0 & !PA_MASK) as usize))
    }

    fn get_leaf(&self) -> Option<PageAttribute> {
        let mut page_attr = PageAttribute::READ;

        if self.0 & PA_P != 0 {
            page_attr |= PageAttribute::PRESENT;
        }

        if self.0 & PA_RW != 0 {
            page_attr |= PageAttribute::WRITE;
        }

        if self.0 & PA_NXE == 0 {
            page_attr |= PageAttribute::EXECUTE;
        }

        if self.0 & PA_US != 0 {
            page_attr |= PageAttribute::USER;
        }

        if self.0 & PA_A != 0 {
            page_attr |= PageAttribute::ACCESSED;
        }

        if self.0 & PA_D != 0 {
            page_attr |= PageAttribute::DIRTY;
        }

        if self.0 & PA_G != 0 {
            page_attr |= PageAttribute::GLOBAL;
        }

        if self.0 & PA_COW != 0 {
            page_attr |= PageAttribute::COPY_ON_WRITE;
        }

        if self.0 & PA_MMAP != 0 {
            page_attr |= PageAttribute::MAPPED;
        }

        if self.0 & PA_ANON != 0 {
            page_attr |= PageAttribute::ANONYMOUS;
        }

        if self.0 & PA_PS != 0 {
            page_attr |= PageAttribute::HUGE;
        }

        Some(page_attr)
    }

    fn get_table(&self) -> Option<TableAttribute> {
        let mut table_attr = TableAttribute::empty();

        if self.0 & PA_PS != 0 {
            return None;
        }

        if self.0 & PA_P != 0 {
            table_attr |= TableAttribute::PRESENT;
        }
        if self.0 & PA_G != 0 {
            table_attr |= TableAttribute::GLOBAL;
        }
        if self.0 & PA_US != 0 {
            table_attr |= TableAttribute::USER;
        }
        if self.0 & PA_A != 0 {
            table_attr |= TableAttribute::ACCESSED;
        }

        Some(table_attr)
    }

    fn set_leaf(&mut self, pfn: PFN, attr: PageAttribute) {
        self.0 = PAddr::from(pfn).addr() as u64 & !PA_MASK;

        // Pages are non-executable unless `EXECUTE` clears `PA_NXE`.
        self.0 |= PA_NXE;

        for attr in attr.iter() {
            match attr {
                PageAttribute::PRESENT => self.0 |= PA_P,
                PageAttribute::READ => {}
                PageAttribute::WRITE => self.0 |= PA_RW,
                PageAttribute::EXECUTE => self.0 &= !PA_NXE,
                PageAttribute::USER => self.0 |= PA_US,
                PageAttribute::ACCESSED => self.0 |= PA_A,
                PageAttribute::DIRTY => self.0 |= PA_D,
                PageAttribute::GLOBAL => self.0 |= PA_G,
                PageAttribute::COPY_ON_WRITE => self.0 |= PA_COW,
                PageAttribute::MAPPED => self.0 |= PA_MMAP,
                PageAttribute::ANONYMOUS => self.0 |= PA_ANON,
                PageAttribute::HUGE => self.0 |= PA_PS,
                _ => unreachable!("Invalid page attribute"),
            }
        }
    }

    fn set_table(&mut self, pfn: PFN, attr: TableAttribute) {
        self.0 = PAddr::from(pfn).addr() as u64 & !PA_MASK;

        // `PA_RW` must be set on every level for a write to reach the leaf.
        self.0 |= PA_RW;

        for attr in attr.iter() {
            match attr {
                TableAttribute::PRESENT => self.0 |= PA_P,
                TableAttribute::GLOBAL => self.0 |= PA_G,
                TableAttribute::USER => self.0 |= PA_US,
                TableAttribute::ACCESSED => self.0 |= PA_A,
                _ => unreachable!("Invalid table attribute"),
            }
        }
    }
}
