use crate::BuddyRawPage;
use core::marker::{PhantomData, Send, Sync};
use intrusive_list::Link;

pub struct FreeArea<T> {
    free_list: Link,
    count: usize,
    _phantom: PhantomData<T>,
}

unsafe impl<T> Send for FreeArea<T> {}
unsafe impl<T> Sync for FreeArea<T> {}

impl<Raw> FreeArea<Raw>
where
    Raw: BuddyRawPage,
{
    pub const fn new() -> Self {
        Self {
            free_list: Link::new(),
            count: 0,
            _phantom: PhantomData,
        }
    }

    pub fn get_free_pages(&mut self) -> Option<Raw> {
        self.free_list.next_mut().map(|pages_link| {
            assert_ne!(self.count, 0);

            let pages_ptr = unsafe {
                // SAFETY: Items in `self.free_list` are guaranteed to be of type `Raw`.
                Raw::from_link(pages_link)
            };

            self.count -= 1;
            pages_link.remove();

            pages_ptr
        })
    }

    pub fn add_pages(&mut self, pages_ptr: Raw) {
        self.count += 1;
        pages_ptr.set_free();

        unsafe {
            self.free_list.insert(pages_ptr.get_link());
        }
    }

    pub fn del_pages(&mut self, pages_ptr: Raw) {
        assert!(self.count >= 1 && pages_ptr.is_free());
        self.count -= 1;
        pages_ptr.clear_free();
        unsafe {
            pages_ptr.get_link().remove();
        }
    }
}
