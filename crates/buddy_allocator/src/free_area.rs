use core::marker::{Send, Sync};
use eonix_mm::paging::{PageFlags, RawPage, RawPagePtr};
use intrusive_list::{container_of, Link};

pub struct FreeArea {
    free_list: Link,
    count: usize,
}

unsafe impl Send for FreeArea {}
unsafe impl Sync for FreeArea {}

impl FreeArea {
    pub const fn new() -> Self {
        Self {
            free_list: Link::new(),
            count: 0,
        }
    }

    pub fn get_free_pages(&mut self) -> Option<RawPagePtr> {
        self.free_list.next_mut().map(|pages_link| {
            assert_ne!(self.count, 0);

            let pages_ptr = unsafe { container_of!(pages_link, RawPage, link) };
            let pages_ptr = RawPagePtr::new(pages_ptr);

            self.count -= 1;
            pages_link.remove();

            pages_ptr
        })
    }

    pub fn add_pages(&mut self, pages_ptr: RawPagePtr) {
        self.count += 1;
        pages_ptr.as_mut().flags.set(PageFlags::FREE);
        self.free_list.insert(&mut pages_ptr.as_mut().link)
    }

    pub fn del_pages(&mut self, pages_ptr: RawPagePtr) {
        assert!(self.count >= 1 && pages_ptr.as_ref().flags.has(PageFlags::FREE));
        self.count -= 1;
        pages_ptr.as_mut().flags.clear(PageFlags::FREE);
        pages_ptr.as_mut().link.remove();
    }
}
