use crate::page_table::{PageAttribute, TableAttribute, PTE};
use crate::paging::PFN;

const PA_V: u64 = 0b1 << 0;
const PA_R: u64 = 0b1 << 1;
const PA_W: u64 = 0b1 << 2;
const PA_X: u64 = 0b1 << 3;
const PA_U: u64 = 0b1 << 4;
const PA_G: u64 = 0b1 << 5;
const PA_A: u64 = 0b1 << 6;
const PA_D: u64 = 0b1 << 7;

// in RSW
const PA_COW: u64 = 0b1 << 8;
const PA_MMAP: u64 = 0b1 << 9;

// Bits 0-9 (V, R, W, X, U, G, A, D, RSW) hold the flags, the PPN follows.
const PA_SHIFT: u64 = 10;

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct RawPTE(u64);

impl PTE for RawPTE {
    fn into_raw(self) -> usize {
        self.0 as usize
    }

    fn from_raw(val: usize) -> Self {
        Self(val as u64)
    }

    fn null() -> Self {
        Self(0)
    }

    fn get_pfn(&self) -> PFN {
        PFN::from(self.0 as usize >> PA_SHIFT)
    }

    fn get_leaf(&self) -> Option<PageAttribute> {
        let mut page_attr = PageAttribute::empty();

        if self.0 & PA_V != 0 {
            page_attr |= PageAttribute::PRESENT;
        }

        // A valid entry with no permission bits is a pointer to next level.
        if page_attr.contains(PageAttribute::PRESENT)
            && (self.0 & (PA_R | PA_W | PA_X) == 0)
        {
            return None;
        }

        if self.0 & PA_R != 0 {
            page_attr |= PageAttribute::READ;
        }

        if self.0 & PA_W != 0 {
            page_attr |= PageAttribute::WRITE;
        }

        if self.0 & PA_X != 0 {
            page_attr |= PageAttribute::EXECUTE;
        }

        if self.0 & PA_U != 0 {
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

        Some(page_attr)
    }

    fn get_table(&self) -> Option<TableAttribute> {
        let mut table_attr = TableAttribute::empty();

        // A valid entry with any permission bit is a leaf.
        if self.0 & PA_V != 0 && self.0 & (PA_R | PA_W | PA_X) != 0 {
            return None;
        }

        if self.0 & PA_V != 0 {
            table_attr |= TableAttribute::PRESENT;
        }
        if self.0 & PA_G != 0 {
            table_attr |= TableAttribute::GLOBAL;
        }
        if self.0 & PA_U != 0 {
            table_attr |= TableAttribute::USER;
        }
        if self.0 & PA_A != 0 {
            table_attr |= TableAttribute::ACCESSED;
        }

        Some(table_attr)
    }

    fn set_leaf(&mut self, pfn: PFN, attr: PageAttribute) {
        self.0 = (usize::from(pfn) as u64) << PA_SHIFT;

        for attr in attr.iter() {
            match attr {
                PageAttribute::PRESENT => self.0 |= PA_V,
                PageAttribute::READ => self.0 |= PA_R,
                PageAttribute::WRITE => self.0 |= PA_W,
                PageAttribute::EXECUTE => self.0 |= PA_X,
                PageAttribute::USER => self.0 |= PA_U,
                PageAttribute::ACCESSED => self.0 |= PA_A,
                PageAttribute::DIRTY => self.0 |= PA_D,
                PageAttribute::GLOBAL => self.0 |= PA_G,
                PageAttribute::COPY_ON_WRITE => self.0 |= PA_COW,
                PageAttribute::MAPPED => self.0 |= PA_MMAP,
                PageAttribute::ANONYMOUS => {}
                _ => unreachable!("Invalid page attribute"),
            }
        }
    }

    fn set_table(&mut self, pfn: PFN, attr: TableAttribute) {
        self.0 = (usize::from(pfn) as u64) << PA_SHIFT;

        for attr in attr.iter() {
            match attr {
                TableAttribute::PRESENT => self.0 |= PA_V,
                TableAttribute::GLOBAL => self.0 |= PA_G,
                TableAttribute::USER | TableAttribute::ACCESSED => {}
                _ => unreachable!("Invalid table attribute"),
            }
        }
    }
}
