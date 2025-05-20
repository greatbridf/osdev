use super::raw_page::RawPagePtr;

pub trait PageAlloc: Sized {
    /// Allocate a page of the given *order*.
    fn alloc_order(order: u32) -> Option<RawPagePtr>;

    /// Allocate exactly one page.
    fn alloc() -> Option<RawPagePtr> {
        Self::alloc_order(0)
    }

    /// Allocate a contiguous block of pages that can contain at least `count` pages.
    fn alloc_at_least(count: usize) -> Option<RawPagePtr> {
        let order = count.next_power_of_two().trailing_zeros();
        Self::alloc_order(order)
    }

    /// Deallocate a page.
    ///
    /// # Safety
    /// This function is unsafe because it assumes that the caller has ensured that
    /// `page` is allocated in this allocator and never used after this call.
    unsafe fn dealloc(page_ptr: RawPagePtr);

    /// Check whether the page is allocated and managed by the allocator.
    ///
    /// # Safety
    /// This function is unsafe because it assumes that the caller has ensured that
    /// `page_ptr` points to a raw page inside the global page array.
    unsafe fn has_management_over(page_ptr: RawPagePtr) -> bool;
}
