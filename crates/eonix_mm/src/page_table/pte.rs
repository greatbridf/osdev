use crate::paging::PFN;

pub trait PageAttribute: Copy {
    /// Create a new instance of the attribute with all attributes set to false.
    fn new() -> Self;

    fn present(self, present: bool) -> Self;
    fn write(self, write: bool) -> Self;
    fn execute(self, execute: bool) -> Self;
    fn user(self, user: bool) -> Self;
    fn accessed(self, accessed: bool) -> Self;
    fn dirty(self, dirty: bool) -> Self;
    fn global(self, global: bool) -> Self;
    fn copy_on_write(self, cow: bool) -> Self;
    fn mapped(self, mmap: bool) -> Self;
    fn anonymous(self, anon: bool) -> Self;

    fn is_present(&self) -> bool;
    fn is_write(&self) -> bool;
    fn is_execute(&self) -> bool;
    fn is_user(&self) -> bool;
    fn is_accessed(&self) -> bool;
    fn is_dirty(&self) -> bool;
    fn is_global(&self) -> bool;
    fn is_copy_on_write(&self) -> bool;
    fn is_mapped(&self) -> bool;
    fn is_anonymous(&self) -> bool;
}

pub trait PTE: Sized {
    type Attr: PageAttribute;

    fn set(&mut self, pfn: PFN, attr: Self::Attr);
    fn get(&self) -> (PFN, Self::Attr);
    fn take(&mut self) -> (PFN, Self::Attr);

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
