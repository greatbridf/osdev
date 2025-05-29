use super::SlabRawPage;
use core::marker::PhantomData;
use eonix_mm::paging::{PageAlloc, PAGE_SIZE};
use intrusive_list::List;

pub(crate) struct SlabCache<T, A> {
    empty_list: List,
    partial_list: List,
    full_list: List,
    object_size: u32,
    alloc: A,
    _phantom: PhantomData<T>,
}

impl<Raw, Allocator> SlabCache<Raw, Allocator>
where
    Raw: SlabRawPage,
    Allocator: PageAlloc<RawPage = Raw>,
{
    pub(crate) const fn new_in(object_size: u32, alloc: Allocator) -> Self {
        // avoid uncessary branch in alloc and dealloc
        assert!(object_size <= PAGE_SIZE as u32 / 2);

        Self {
            empty_list: List::new(),
            partial_list: List::new(),
            full_list: List::new(),
            alloc,
            object_size: object_size,
            _phantom: PhantomData,
        }
    }

    pub(crate) fn alloc(&mut self) -> *mut u8 {
        if !self.partial_list.is_empty() {
            let page_ptr = unsafe {
                Raw::from_link(
                    self.partial_list
                        .head()
                        .expect("partial pages should not be empty"),
                )
            };

            let ptr = page_ptr.alloc_slot();
            if page_ptr.is_full() {
                self.partial_list.remove(unsafe { page_ptr.get_link() });
                self.full_list.insert(unsafe { page_ptr.get_link() });
            }
            return ptr;
        }

        if !self.empty_list.is_empty() {
            let page_ptr = unsafe {
                Raw::from_link(
                    self.empty_list
                        .head()
                        .expect("empty pages should not be empty"),
                )
            };

            let ptr = page_ptr.alloc_slot();
            self.empty_list.remove(unsafe { page_ptr.get_link() });
            self.partial_list.insert(unsafe { page_ptr.get_link() });
            return ptr;
        }

        let new_page = self.alloc.alloc().expect("slab_cache get page fail!");
        new_page.slab_init(self.object_size);
        let ptr = new_page.alloc_slot();
        self.partial_list.insert(unsafe { new_page.get_link() });
        ptr
    }

    pub(crate) fn dealloc(&mut self, ptr: *mut u8) {
        let page_ptr = Raw::in_which(ptr);

        if page_ptr.is_full() {
            self.full_list.remove(unsafe { page_ptr.get_link() });
            self.partial_list.insert(unsafe { page_ptr.get_link() });
        }

        page_ptr.dealloc_slot(ptr);

        if page_ptr.is_emtpy() {
            self.partial_list.remove(unsafe { page_ptr.get_link() });
            self.empty_list.insert(unsafe { page_ptr.get_link() });
        }
    }
}
