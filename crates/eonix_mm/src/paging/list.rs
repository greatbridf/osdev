pub trait PageList {
    type Page;

    fn is_empty(&self) -> bool;

    fn peek_head(&mut self) -> Option<&mut Self::Page>;

    fn pop_head(&mut self) -> Option<&'static mut Self::Page>;
    fn push_tail(&mut self, page: &'static mut Self::Page);
    fn remove(&mut self, page: &mut Self::Page);
}

pub trait PageListSized: PageList + Sized {
    const NEW: Self;

    fn new() -> Self {
        Self::NEW
    }
}
