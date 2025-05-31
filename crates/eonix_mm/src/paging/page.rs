use super::{GlobalPageAlloc, PageAlloc, RawPage as _, PFN};
use crate::address::{AddrRange, PAddr};
use core::{fmt, mem::ManuallyDrop, ptr::NonNull, sync::atomic::Ordering};

pub const PAGE_SIZE: usize = 4096;
pub const PAGE_SIZE_BITS: u32 = PAGE_SIZE.trailing_zeros();

/// A block of memory that is aligned to the page size and can be used for
/// page-aligned allocations.
///
/// This is used to ensure that the memory is properly aligned to the page size.
#[allow(dead_code)]
#[repr(align(4096))]
pub struct PageBlock([u8; PAGE_SIZE]);

/// A trait that provides the kernel access to the page.
#[doc(notable_trait)]
pub trait PageAccess {
    /// Returns a kernel-accessible pointer to the page referenced by the given
    /// physical frame number.
    ///
    /// # Safety
    /// This function is unsafe because calling this function on some non-existing
    /// pfn will cause undefined behavior.
    unsafe fn get_ptr_for_pfn(pfn: PFN) -> NonNull<PageBlock>;

    /// Returns a kernel-accessible pointer to the given page.
    fn get_ptr_for_page<A: PageAlloc>(page: &Page<A>) -> NonNull<PageBlock> {
        unsafe {
            // SAFETY: `page.pfn()` is guaranteed to be valid.
            Self::get_ptr_for_pfn(page.pfn())
        }
    }
}

/// A Page allocated in allocator `A`.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct Page<A: PageAlloc> {
    raw_page: A::RawPage,
    alloc: A,
}

unsafe impl<A: PageAlloc> Send for Page<A> {}
unsafe impl<A: PageAlloc> Sync for Page<A> {}

impl<A> Page<A>
where
    A: GlobalPageAlloc,
{
    /// Allocate a page of the given *order*.
    pub fn alloc_order(order: u32) -> Self {
        Self::alloc_order_in(order, A::global())
    }

    /// Allocate exactly one page.
    pub fn alloc() -> Self {
        Self::alloc_in(A::global())
    }

    /// Allocate a contiguous block of pages that can contain at least `count` pages.
    pub fn alloc_at_least(count: usize) -> Self {
        Self::alloc_at_least_in(count, A::global())
    }

    /// Acquire the ownership of the page pointed to by `pfn`, leaving `refcount` untouched.
    ///
    /// # Safety
    /// This function is unsafe because it assumes that the caller has ensured that
    /// `pfn` points to a valid page allocated through `alloc_order()` and that the
    /// page have not been freed or deallocated yet.
    ///
    /// No checks are done. Any violation of this assumption may lead to undefined behavior.
    pub unsafe fn from_raw_unchecked(pfn: PFN) -> Self {
        unsafe { Self::from_raw_unchecked_in(pfn, A::global()) }
    }

    /// Acquire the ownership of the page pointed to by `pfn`, leaving `refcount` untouched.
    ///
    /// This function is a safe wrapper around `from_paddr_unchecked()` that does **some sort
    /// of** checks to ensure that the page is valid and managed by the allocator.
    ///
    /// # Panic
    /// This function will panic if the page is not valid or if the page is not managed by
    /// the allocator.
    ///
    /// # Safety
    /// This function is unsafe because it assumes that the caller has ensured that
    /// `pfn` points to an existing page (A.K.A. inside the global page array) and the
    /// page will not be freed or deallocated during the call.
    pub unsafe fn from_raw(pfn: PFN) -> Self {
        unsafe { Self::from_raw_in(pfn, A::global()) }
    }

    /// Do some work with the page without touching the reference count with the same
    /// restrictions as `from_raw_in()`.
    ///
    /// # Safety
    /// Check `from_raw()` for the safety requirements.
    pub unsafe fn with_raw<F, O>(pfn: PFN, func: F) -> O
    where
        F: FnOnce(&Self) -> O,
    {
        unsafe { Self::with_raw_in(pfn, A::global(), func) }
    }

    /// Do some work with the page without touching the reference count with the same
    /// restrictions as `from_raw_unchecked_in()`.
    ///
    /// # Safety
    /// Check `from_raw_unchecked()` for the safety requirements.
    pub unsafe fn with_raw_unchecked<F, O>(pfn: PFN, func: F, alloc: A) -> O
    where
        F: FnOnce(&Self) -> O,
    {
        unsafe { Self::with_raw_unchecked_in(pfn, func, alloc) }
    }
}

impl<A> Page<A>
where
    A: PageAlloc,
{
    /// Allocate a page of the given *order*.
    pub fn alloc_order_in(order: u32, alloc: A) -> Self {
        Self {
            raw_page: alloc.alloc_order(order).expect("Out of memory"),
            alloc,
        }
    }

    /// Allocate exactly one page.
    pub fn alloc_in(alloc: A) -> Self {
        Self {
            raw_page: alloc.alloc().expect("Out of memory"),
            alloc,
        }
    }

    /// Allocate a contiguous block of pages that can contain at least `count` pages.
    pub fn alloc_at_least_in(count: usize, alloc: A) -> Self {
        Self {
            raw_page: alloc.alloc_at_least(count).expect("Out of memory"),
            alloc,
        }
    }

    /// Acquire the ownership of the page pointed to by `pfn`, leaving `refcount` untouched.
    ///
    /// # Safety
    /// This function is unsafe because it assumes that the caller has ensured that
    /// `pfn` points to a valid page managed by `alloc` and that the page have not
    /// been freed or deallocated yet.
    ///
    /// No checks are done. Any violation of this assumption may lead to undefined behavior.
    pub unsafe fn from_raw_unchecked_in(pfn: PFN, alloc: A) -> Self {
        Self {
            raw_page: A::RawPage::from(pfn),
            alloc,
        }
    }

    /// Acquire the ownership of the page pointed to by `pfn`, leaving `refcount` untouched.
    ///
    /// This function is a safe wrapper around `from_paddr_unchecked()` that does **some sort
    /// of** checks to ensure that the page is valid and managed by the allocator.
    ///
    /// # Panic
    /// This function will panic if the page is not valid or if the page is not managed by
    /// the allocator.
    ///
    /// # Safety
    /// This function is unsafe because it assumes that the caller has ensured that
    /// `pfn` points to an existing page (A.K.A. inside the global page array) and the
    /// page will not be freed or deallocated during the call.
    pub unsafe fn from_raw_in(pfn: PFN, alloc: A) -> Self {
        unsafe {
            // SAFETY: The caller guarantees that the page is inside the global page array.
            assert!(alloc.has_management_over(A::RawPage::from(pfn)));

            // SAFETY: We've checked that the validity of the page. And the caller guarantees
            //         that the page will not be freed or deallocated during the call.
            Self::from_raw_unchecked_in(pfn, alloc)
        }
    }

    /// Do some work with the page without touching the reference count with the same
    /// restrictions as `from_raw_in()`.
    ///
    /// # Safety
    /// Check `from_raw_in()` for the safety requirements.
    pub unsafe fn with_raw_in<F, O>(pfn: PFN, alloc: A, func: F) -> O
    where
        F: FnOnce(&Self) -> O,
    {
        unsafe {
            let me = ManuallyDrop::new(Self::from_raw_in(pfn, alloc));
            func(&me)
        }
    }

    /// Do some work with the page without touching the reference count with the same
    /// restrictions as `from_raw_unchecked_in()`.
    ///
    /// # Safety
    /// Check `from_raw_unchecked_in()` for the safety requirements.
    pub unsafe fn with_raw_unchecked_in<F, O>(pfn: PFN, func: F, alloc: A) -> O
    where
        F: FnOnce(&Self) -> O,
    {
        unsafe {
            let me = ManuallyDrop::new(Self::from_raw_unchecked_in(pfn, alloc));
            func(&me)
        }
    }

    /// Whether we are the only owner of the page.
    pub fn is_exclusive(&self) -> bool {
        self.raw_page.refcount().load(Ordering::Acquire) == 1
    }

    /// Returns the *order* of the page, which is the log2 of the number of pages
    /// contained in the page object.
    pub fn order(&self) -> u32 {
        self.raw_page.order()
    }

    /// Returns the total size of the page in bytes.
    pub fn len(&self) -> usize {
        1 << (self.order() + PAGE_SIZE_BITS)
    }

    /// Consumes the `Page` and returns the physical frame number without dropping
    /// the reference count the page holds.
    pub fn into_raw(self) -> PFN {
        let me = ManuallyDrop::new(self);
        me.pfn()
    }

    /// Returns the physical frame number of the page, which is aligned with the
    /// page size and valid.
    pub fn pfn(&self) -> PFN {
        Into::<PFN>::into(self.raw_page)
    }

    /// Returns the start physical address of the page, which is guaranteed to be
    /// aligned to the page size and valid.
    pub fn start(&self) -> PAddr {
        PAddr::from(self.pfn())
    }

    /// Returns the physical address range of the page, which is guaranteed to be
    /// aligned to the page size and valid.
    pub fn range(&self) -> AddrRange<PAddr> {
        AddrRange::from(self.start()).grow(self.len())
    }

    /// Get the allocator that manages this page.
    pub fn allocator(&self) -> &A {
        &self.alloc
    }
}

impl<A> Clone for Page<A>
where
    A: PageAlloc,
{
    fn clone(&self) -> Self {
        // SAFETY: Memory order here can be Relaxed is for the same reason as that
        // in the copy constructor of `std::shared_ptr`.
        self.raw_page.refcount().fetch_add(1, Ordering::Relaxed);

        Self {
            raw_page: self.raw_page,
            alloc: self.alloc.clone(),
        }
    }
}

impl<A> Drop for Page<A>
where
    A: PageAlloc,
{
    fn drop(&mut self) {
        match self.raw_page.refcount().fetch_sub(1, Ordering::AcqRel) {
            0 => panic!("Refcount for an in-use page is 0"),
            1 => unsafe {
                // SAFETY: `self.raw_page` points to a valid page inside the global page array.
                assert!(self.alloc.has_management_over(self.raw_page));

                // SAFETY: `self.raw_page` is managed by the allocator and we're dropping the page.
                self.alloc.dealloc(self.raw_page)
            },
            _ => {}
        }
    }
}

impl<A: PageAlloc> fmt::Debug for Page<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Page({:?}, order={})",
            Into::<PFN>::into(self.raw_page),
            self.order()
        )
    }
}
