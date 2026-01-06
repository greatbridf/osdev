#![no_std]

use core::ptr::NonNull;

use eonix_mm::paging::{PageList, PageListSized};
use eonix_sync::Spin;

#[repr(C)]
pub union SlabSlot {
    slab_slot: Option<NonNull<SlabSlot>>,
    data: u8,
}

pub trait SlabPage: Sized + 'static {
    fn get_data_ptr(&self) -> NonNull<[u8]>;

    fn get_free_slot(&self) -> Option<NonNull<SlabSlot>>;
    fn set_free_slot(&mut self, next: Option<NonNull<SlabSlot>>);

    fn get_alloc_count(&self) -> usize;

    /// Increase the allocation count by 1 and return the increased value.
    fn inc_alloc_count(&mut self) -> usize;

    /// Decrease the allocation count by 1 and return the decreased value.
    fn dec_alloc_count(&mut self) -> usize;

    /// Get the [`SlabPage`] that `ptr` is allocated from.
    ///
    /// # Safety
    /// The caller MUST ensure that no others could be calling this function and
    /// getting the [`SlabPage`] at the same time.
    unsafe fn from_allocated(ptr: NonNull<u8>) -> &'static mut Self;
}

pub(crate) trait SlabPageExt {
    fn alloc_slot(&mut self) -> Option<NonNull<u8>>;

    /// # Safety
    /// The caller MUST ensure that `slot_data_ptr` points to some position
    /// previously allocated by [`SlabPageExt::alloc_slot`].
    unsafe fn free_slot(&mut self, slot_data_ptr: NonNull<u8>);

    fn is_empty(&self) -> bool;
    fn is_full(&self) -> bool;
}

impl<T> SlabPageExt for T
where
    T: SlabPage,
{
    fn alloc_slot(&mut self) -> Option<NonNull<u8>> {
        let mut free_slot = self.get_free_slot()?;

        unsafe {
            let free_slot = free_slot.as_mut();

            let next_slot = free_slot.slab_slot;
            // ===== `free_slot` is now safe to be overwritten

            self.set_free_slot(next_slot);
            self.inc_alloc_count();

            Some(NonNull::new_unchecked(&mut free_slot.data))
        }
    }

    unsafe fn free_slot(&mut self, slot_data_ptr: NonNull<u8>) {
        unsafe {
            let mut free_slot: NonNull<SlabSlot> = slot_data_ptr.cast();
            free_slot.as_mut().slab_slot = self.get_free_slot();

            self.set_free_slot(Some(free_slot));
            self.dec_alloc_count();
        }
    }

    fn is_empty(&self) -> bool {
        self.get_alloc_count() == 0
    }

    fn is_full(&self) -> bool {
        self.get_free_slot().is_none()
    }
}

pub trait SlabPageAlloc {
    type Page: SlabPage;
    type PageList: PageList<Page = Self::Page>;

    /// Allocate a page suitable for slab system use. The page MUST come with
    /// its allocation count 0 and next free slot None.
    ///
    /// # Safety
    /// The page returned MUST be properly initialized before its usage.
    unsafe fn alloc_uninit(&self) -> &'static mut Self::Page;
}

pub(crate) struct SlabList<T>
where
    T: PageList,
{
    empty_list: T,
    partial_list: T,
    full_list: T,
    object_size: usize,
}

pub struct SlabAlloc<P, const COUNT: usize>
where
    P: SlabPageAlloc,
{
    slabs: [Spin<SlabList<P::PageList>>; COUNT],
    alloc: P,
}

unsafe impl<P, const COUNT: usize> Send for SlabAlloc<P, COUNT> where P: SlabPageAlloc {}
unsafe impl<P, const COUNT: usize> Sync for SlabAlloc<P, COUNT> where P: SlabPageAlloc {}

impl<L, const COUNT: usize> SlabAlloc<L, COUNT>
where
    L: SlabPageAlloc,
    L::PageList: PageListSized,
{
    pub fn new_in(alloc: L) -> Self {
        Self {
            slabs: core::array::from_fn(|i| Spin::new(SlabList::new(1 << (i + 3)))),
            alloc,
        }
    }

    pub fn alloc(&self, mut size: usize) -> NonNull<u8> {
        size = size.max(8);
        let idx = size.next_power_of_two().trailing_zeros() - 3;
        self.slabs[idx as usize].lock().alloc(&self.alloc)
    }

    pub unsafe fn dealloc(&self, ptr: NonNull<u8>, mut size: usize) {
        size = size.max(8);
        let idx = size.next_power_of_two().trailing_zeros() - 3;

        unsafe {
            // SAFETY:
            self.slabs[idx as usize].lock().dealloc(ptr, &self.alloc);
        }
    }
}

impl<T> SlabList<T>
where
    T: PageListSized,
{
    const fn new(object_size: usize) -> Self {
        Self {
            empty_list: T::NEW,
            partial_list: T::NEW,
            full_list: T::NEW,
            object_size,
        }
    }
}

impl<T> SlabList<T>
where
    T: PageList,
    T::Page: SlabPage,
{
    fn alloc_from_partial(&mut self) -> NonNull<u8> {
        let head = self.partial_list.peek_head().unwrap();
        let slot = head.alloc_slot().unwrap();

        if head.is_full() {
            let head = self.partial_list.pop_head().unwrap();
            self.full_list.push_tail(head);
        }

        slot
    }

    fn alloc_from_empty(&mut self) -> NonNull<u8> {
        let head = self.empty_list.pop_head().unwrap();
        let slot = head.alloc_slot().unwrap();

        if head.is_full() {
            self.full_list.push_tail(head);
        } else {
            self.partial_list.push_tail(head);
        }

        slot
    }

    fn charge(&mut self, alloc: &impl SlabPageAlloc<Page = T::Page>) {
        unsafe {
            let slab = alloc.alloc_uninit();
            let free_slot = make_slab_page(slab.get_data_ptr(), self.object_size);

            slab.set_free_slot(Some(free_slot));

            self.empty_list.push_tail(slab);
        }
    }

    fn alloc(&mut self, alloc: &impl SlabPageAlloc<Page = T::Page>) -> NonNull<u8> {
        if !self.partial_list.is_empty() {
            return self.alloc_from_partial();
        }

        if self.empty_list.is_empty() {
            self.charge(alloc);
        }

        self.alloc_from_empty()
    }

    unsafe fn dealloc(&mut self, ptr: NonNull<u8>, _alloc: &impl SlabPageAlloc) {
        let slab_page = unsafe {
            // SAFETY:
            <T::Page>::from_allocated(ptr)
        };

        let (was_full, is_empty);

        was_full = slab_page.is_full();

        unsafe {
            // SAFETY:
            slab_page.free_slot(ptr);
        }

        is_empty = slab_page.is_empty();

        match (was_full, is_empty) {
            (false, false) => {}
            (false, true) => {
                self.partial_list.remove(slab_page);
                self.empty_list.push_tail(slab_page);
            }
            (true, false) => {
                self.full_list.remove(slab_page);
                self.partial_list.push_tail(slab_page);
            }
            (true, true) => {
                self.full_list.remove(slab_page);
                self.empty_list.push_tail(slab_page);
            }
        }

        // TODO: Check whether we should place some pages back with `alloc` if
        //       the global free page count is below the watermark.
    }
}

pub fn make_slab_page(page_ptr: NonNull<[u8]>, slot_size: usize) -> NonNull<SlabSlot> {
    assert!(
        slot_size >= core::mem::size_of::<usize>(),
        "The minimum slot size is of a pointer's width"
    );

    let page_size = page_ptr.len();
    let slot_count = page_size / slot_size;
    let page_start: NonNull<u8> = page_ptr.cast();

    // Quick checks
    assert!(
        page_size % slot_size == 0,
        "The page's size should be a multiple of the slot size"
    );

    let mut prev_free_slot = None;
    for i in (0..slot_count).rev() {
        let offset = i * slot_size;

        unsafe {
            let mut slot_ptr: NonNull<SlabSlot> = page_start.add(offset).cast();

            slot_ptr.as_mut().slab_slot = prev_free_slot;
            prev_free_slot = Some(slot_ptr);
        }
    }

    prev_free_slot.expect("There should be at least one slot.")
}
