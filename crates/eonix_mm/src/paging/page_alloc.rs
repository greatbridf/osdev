use super::Folio;

/// A trait for allocating and deallocating folios.
///
/// Note that the instances of this trait should provide pointer-like or reference-like
/// behavior, meaning that the allocators are to be passed around by value and stored in
/// managed data structures. This is because the allocator may be used to deallocate the
/// pages it allocates.
pub trait FrameAlloc: Clone {
    type Folio: Folio;

    /// Allocate a folio of the given *order*.
    fn alloc_order(&self, order: u32) -> Option<Self::Folio>;

    /// Allocate exactly one folio.
    fn alloc(&self) -> Option<Self::Folio> {
        self.alloc_order(0)
    }

    /// Allocate a folio that can contain at least [`count`] contiguous pages.
    fn alloc_at_least(&self, count: usize) -> Option<Self::Folio> {
        let order = count.next_power_of_two().trailing_zeros();
        self.alloc_order(order)
    }
}

/// A trait for global page allocators.
///
/// Global means that we can get an instance of the allocator from anywhere in the kernel.
pub trait GlobalFrameAlloc: FrameAlloc + 'static {
    /// The global page allocator.
    const GLOBAL: Self;
}

impl<'a, A> FrameAlloc for &'a A
where
    A: FrameAlloc,
{
    type Folio = A::Folio;

    fn alloc_order(&self, order: u32) -> Option<Self::Folio> {
        (*self).alloc_order(order)
    }
}
