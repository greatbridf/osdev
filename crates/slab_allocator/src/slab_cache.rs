use super::SlabRawPage;
use core::{marker::PhantomData, ptr::NonNull};
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

trait SlabRawPageExt {
    fn alloc_slot(&self) -> Option<NonNull<usize>>;
    fn dealloc_slot(&self, slot_ptr: *mut u8);
    fn is_full(&self) -> bool;
    fn is_empty(&self) -> bool;
    fn slab_page_init(&self, object_size: u32) -> Option<NonNull<usize>>;
}

impl<T> SlabRawPageExt for T
where
    T: SlabRawPage,
{
    fn alloc_slot(&self) -> Option<NonNull<usize>> {
        let ptr = self.next_free().clone();

        let next_free = match ptr {
            Some(ptr) => unsafe { ptr.read() as *mut usize },
            None => unreachable!(),
        };
        *self.allocated_count() += 1;
        *self.next_free() = NonNull::new(next_free);
        return ptr;
    }

    fn dealloc_slot(&self, slot_ptr: *mut u8) {
        let slot_ptr = slot_ptr as *mut usize;

        if let Some(last_free) = self.next_free().clone() {
            unsafe { *slot_ptr = last_free.as_ptr() as usize }
        } else {
            unsafe { *slot_ptr = 0 }
        }

        *self.allocated_count() -= 1;
        *self.next_free() = NonNull::new(slot_ptr);
    }

    fn slab_page_init(&self, object_size: u32) -> Option<NonNull<usize>> {
        assert!(object_size >= core::mem::size_of::<usize>() as u32);

        let first_free = self.real_page_ptr() as *mut usize;

        let mut slot_ptr = first_free;
        let mut slot_count = PAGE_SIZE / object_size as usize;

        // SAFETY: carefully ptr operate
        unsafe {
            loop {
                if slot_count == 1 {
                    *slot_ptr = 0;
                    break;
                }

                let next_ptr = slot_ptr.byte_add(object_size as usize);
                *slot_ptr = next_ptr as usize;
                slot_ptr = next_ptr;
                slot_count -= 1;
            }
        }

        NonNull::new(first_free)
    }

    fn is_empty(&self) -> bool {
        self.allocated_count().clone() == 0
    }

    fn is_full(&self) -> bool {
        self.next_free().is_none()
    }
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

            let ptr = page_ptr.alloc_slot().expect("should get slot");

            if page_ptr.is_full() {
                self.partial_list.remove(unsafe { page_ptr.get_link() });
                self.full_list.insert(unsafe { page_ptr.get_link() });
            }
            return ptr.as_ptr() as *mut u8;
        }

        if !self.empty_list.is_empty() {
            let page_ptr = unsafe {
                Raw::from_link(
                    self.empty_list
                        .head()
                        .expect("empty pages should not be empty"),
                )
            };

            let ptr = page_ptr.alloc_slot().expect("should get slot");
            self.empty_list.remove(unsafe { page_ptr.get_link() });
            self.partial_list.insert(unsafe { page_ptr.get_link() });
            return ptr.as_ptr() as *mut u8;
        }

        let new_page_ptr = self.alloc.alloc().expect("slab_cache get page fail!");
        let first_free = new_page_ptr.slab_page_init(self.object_size);
        new_page_ptr.slab_init(first_free);
        let ptr = new_page_ptr.alloc_slot().expect("should get slot");
        self.partial_list.insert(unsafe { new_page_ptr.get_link() });
        ptr.as_ptr() as *mut u8
    }

    pub(crate) fn dealloc(&mut self, ptr: *mut u8) {
        let page_ptr = Raw::in_which(ptr);

        if page_ptr.is_full() {
            self.full_list.remove(unsafe { page_ptr.get_link() });
            self.partial_list.insert(unsafe { page_ptr.get_link() });
        }

        page_ptr.dealloc_slot(ptr);

        if page_ptr.is_empty() {
            self.partial_list.remove(unsafe { page_ptr.get_link() });
            self.empty_list.insert(unsafe { page_ptr.get_link() });
        }
    }
}
