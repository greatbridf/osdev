use crate::page_table::{PageAttribute, TableAttribute, PTE};
use crate::paging::PFN;

const PA_VP: u64 = (1 << 0) | (1 << 7);
const PA_D: u64 = 1 << 1;
const PA_U: u64 = 3 << 2;
const PA_CACHED: u64 = 1 << 4;
const PA_G: u64 = 1 << 6;
const PA_W: u64 = 1 << 8;
const PA_NR: u64 = 1 << 61;
const PA_NX: u64 = 1 << 62;

// in RSW
const PA_COW: u64 = 1 << 9;
const PA_MMAP: u64 = 1 << 10;

const PA_PT_USER: u64 = 1 << 59;
const PA_PT: u64 = 1 << 60;

const PA_FLAGS_MASK: u64 = 0xF800_0000_0000_0FFF;

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct RawPTE(u64);

impl PTE for RawPTE {
    fn get_pfn(&self) -> PFN {
        PFN::from((self.0 & !PA_FLAGS_MASK) as usize >> 12)
    }

    fn get_leaf(&self) -> Option<PageAttribute> {
        // `PA_PT` is set on entries pointing to a lower level page table.
        if self.0 & PA_PT == PA_PT {
            return None;
        }

        let mut page_attr = PageAttribute::empty();

        if self.0 & PA_VP == PA_VP {
            page_attr |= PageAttribute::PRESENT;
        }

        if self.0 & PA_NR == 0 {
            page_attr |= PageAttribute::READ;
        }

        if self.0 & PA_W != 0 {
            page_attr |= PageAttribute::WRITE;
        }

        if self.0 & PA_NX == 0 {
            page_attr |= PageAttribute::EXECUTE;
        }

        if self.0 & PA_U == PA_U {
            page_attr |= PageAttribute::USER;
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

        Some(page_attr)
    }

    fn get_table(&self) -> Option<TableAttribute> {
        // A valid entry without `PA_PT` points to a page instead of a table.
        if self.0 & PA_VP == PA_VP && self.0 & PA_PT != PA_PT {
            return None;
        }

        let mut table_attr = TableAttribute::empty();

        if self.0 & PA_PT == PA_PT {
            table_attr |= TableAttribute::PRESENT;
        }

        if self.0 & PA_PT_USER == PA_PT_USER {
            table_attr |= TableAttribute::USER;
        }

        Some(table_attr)
    }

    fn set_leaf(&mut self, pfn: PFN, attr: PageAttribute) {
        self.0 = ((usize::from(pfn) as u64) << 12) & !PA_FLAGS_MASK;

        // Writes are disabled unless `WRITE` adds `PA_W`, and pages are
        // non-readable/non-executable unless `READ`/`EXECUTE` clears `PA_NR`
        // and `PA_NX`. Mapping is cached by default.
        self.0 |= PA_NR | PA_NX | PA_CACHED;

        for attr in attr.iter() {
            match attr {
                PageAttribute::PRESENT => self.0 |= PA_VP,
                PageAttribute::READ => self.0 &= !PA_NR,
                PageAttribute::WRITE => self.0 |= PA_W,
                PageAttribute::EXECUTE => self.0 &= !PA_NX,
                PageAttribute::USER => self.0 |= PA_U,
                PageAttribute::DIRTY => self.0 |= PA_D,
                PageAttribute::GLOBAL => self.0 |= PA_G,
                PageAttribute::COPY_ON_WRITE => self.0 |= PA_COW,
                PageAttribute::MAPPED => self.0 |= PA_MMAP,
                PageAttribute::ACCESSED | PageAttribute::ANONYMOUS => {}
                _ => unreachable!("Invalid page attribute"),
            }
        }
    }

    fn set_table(&mut self, pfn: PFN, attr: TableAttribute) {
        self.0 = ((usize::from(pfn) as u64) << 12) & !PA_FLAGS_MASK;

        for attr in attr.iter() {
            match attr {
                TableAttribute::PRESENT => self.0 |= PA_PT,
                TableAttribute::USER => self.0 |= PA_PT_USER,
                TableAttribute::GLOBAL | TableAttribute::ACCESSED => {}
                _ => unreachable!("Invalid table attribute"),
            }
        }
    }
}
