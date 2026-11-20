use bitflags::bitflags;

use crate::paging::PFN;

bitflags! {
    #[derive(Clone, Copy, PartialEq)]
    pub struct TableAttribute: usize {
        const PRESENT = 1;
        const USER = 2;
        const ACCESSED = 4;
        const GLOBAL = 8;
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct PageAttribute: usize {
        const PRESENT = 1;
        const READ = 2;
        const WRITE = 4;
        const EXECUTE = 8;
        const USER = 16;
        const ACCESSED = 32;
        const DIRTY = 64;
        const GLOBAL = 128;
        const COPY_ON_WRITE = 256;
        const MAPPED = 512;
        const ANONYMOUS = 1024;
        const HUGE = 2048;
    }
}

pub trait PTE: Sized {
    fn get_pfn(&self) -> PFN;
    fn get_leaf(&self) -> Option<PageAttribute>;
    fn get_table(&self) -> Option<TableAttribute>;

    fn set_leaf(&mut self, pfn: PFN, attr: PageAttribute);
    fn set_table(&mut self, pfn: PFN, attr: TableAttribute);

    fn clear(&mut self) {
        self.set_leaf(PFN::from_val(0), PageAttribute::empty());
    }
}
