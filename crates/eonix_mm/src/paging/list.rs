pub trait FolioList {
    type Folio;

    fn is_empty(&self) -> bool;

    fn peek_head(&mut self) -> Option<&mut Self::Folio>;

    fn pop_head(&mut self) -> Option<&'static mut Self::Folio>;
    fn push_tail(&mut self, page: &'static mut Self::Folio);
    fn remove(&mut self, page: &mut Self::Folio);
}

pub trait FolioListSized: FolioList + Sized {
    const NEW: Self;

    fn new() -> Self {
        Self::NEW
    }
}
