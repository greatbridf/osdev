use super::{raw_page::UnmanagedRawPage, RawPage};

/// A trait for allocating and deallocating pages of memory.
///
/// Note that the instances of this trait should provide pointer-like or reference-like
/// behavior, meaning that the allocators are to be passed around by value and stored in
/// managed data structures. This is because the allocator may be used to deallocate the
/// pages it allocates.
#[doc(notable_trait)]
pub trait PageAlloc: Clone {
    type RawPage: RawPage;

    /// Allocate a page of the given *order*.
    fn alloc_order(&self, order: u32) -> Option<Self::RawPage>;

    /// Allocate exactly one page.
    fn alloc(&self) -> Option<Self::RawPage> {
        self.alloc_order(0)
    }

    /// Allocate a contiguous block of pages that can contain at least `count` pages.
    fn alloc_at_least(&self, count: usize) -> Option<Self::RawPage> {
        let order = count.next_power_of_two().trailing_zeros();
        self.alloc_order(order)
    }

    /// Deallocate a page.
    ///
    /// # Safety
    /// This function is unsafe because it assumes that the caller MUST ensure that
    /// `raw_page` is allocated in this allocator and never used after this call.
    unsafe fn dealloc(&self, raw_page: Self::RawPage);

    /// Check whether the page is allocated and managed by the allocator.
    fn has_management_over(&self, page_ptr: Self::RawPage) -> bool;
}

/// A trait for global page allocators.
///
/// Global means that we can get an instance of the allocator from anywhere in the kernel.
#[doc(notable_trait)]
pub trait GlobalPageAlloc: PageAlloc + 'static {
    /// Get the global page allocator.
    fn global() -> Self;
}

#[derive(Clone)]
pub struct NoAlloc;

impl<'a, A> PageAlloc for &'a A
where
    A: PageAlloc,
{
    type RawPage = A::RawPage;

    fn alloc_order(&self, order: u32) -> Option<Self::RawPage> {
        (*self).alloc_order(order)
    }

    unsafe fn dealloc(&self, raw_page: Self::RawPage) {
        unsafe { (*self).dealloc(raw_page) }
    }

    fn has_management_over(&self, raw_page: Self::RawPage) -> bool {
        (*self).has_management_over(raw_page)
    }
}

impl PageAlloc for NoAlloc {
    type RawPage = UnmanagedRawPage;

    fn alloc_order(&self, _: u32) -> Option<Self::RawPage> {
        panic!("`NoAlloc` cannot allocate pages");
    }

    unsafe fn dealloc(&self, _: Self::RawPage) {
        panic!("`NoAlloc` cannot free pages");
    }

    fn has_management_over(&self, _: Self::RawPage) -> bool {
        true
    }
}

impl GlobalPageAlloc for NoAlloc {
    fn global() -> Self {
        Self
    }
}
