use crate::paging::PFN;
use bitflags::bitflags;

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

#[doc(notable_trait)]
pub trait RawAttribute: Copy + From<PageAttribute> + From<TableAttribute> {
    /// Create a new attribute representing a non-present page.
    fn null() -> Self;

    /// Interpret the attribute as a page table attribute. Return `None` if it is
    /// not an attribute for a page table.
    ///
    /// # Panic
    /// The implementor should panic if invalid combinations of flags are present.
    fn as_table_attr(self) -> Option<TableAttribute>;

    /// Interpret the attribute as a page attribute. Return `None` if it is not
    /// an attribute for a page.
    ///
    /// # Panic
    /// The implementor should panic if invalid combinations of flags are present.
    fn as_page_attr(self) -> Option<PageAttribute>;
}

#[doc(notable_trait)]
pub trait PTE: Sized {
    type Attr: RawAttribute;

    fn set(&mut self, pfn: PFN, attr: Self::Attr);
    fn get(&self) -> (PFN, Self::Attr);

    fn take(&mut self) -> (PFN, Self::Attr) {
        let pfn_attr = self.get();
        self.set(PFN::from_val(0), Self::Attr::null());
        pfn_attr
    }

    fn set_pfn(&mut self, pfn: PFN) {
        self.set(pfn, self.get_attr());
    }

    fn set_attr(&mut self, attr: Self::Attr) {
        self.set(self.get_pfn(), attr);
    }

    fn get_pfn(&self) -> PFN {
        self.get().0
    }

    fn get_attr(&self) -> Self::Attr {
        self.get().1
    }
}
