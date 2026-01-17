use core::fmt;
use core::mem::ManuallyDrop;
use core::ops::Deref;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;

use eonix_mm::paging::{Folio as FolioTrait, FrameAlloc, GlobalFrameAlloc, Zone, PFN};

use super::page_alloc::ZONE;
use super::{GlobalPageAlloc, PhysAccess as _, RawPage};

#[repr(transparent)]
pub struct Folio(NonNull<RawPage>);

#[derive(Debug)]
#[repr(transparent)]
pub struct FolioOwned(Folio);

#[repr(transparent)]
pub struct LockedFolio<'a>(&'a Folio);

unsafe impl Send for Folio {}
unsafe impl Sync for Folio {}

impl Folio {
    pub(super) const fn from_mut_page(raw_page: &'static mut RawPage) -> Self {
        Self(NonNull::new(raw_page).unwrap())
    }

    /// Allocate a folio of the given *order*.
    pub fn alloc_order(order: u32) -> Self {
        GlobalPageAlloc::GLOBAL
            .alloc_order(order)
            .expect("Out of memory")
    }

    /// Allocate a folio of order 0
    pub fn alloc() -> Self {
        Self::alloc_order(0)
    }

    /// Allocate a folio consisting of at least [`count`] pages.
    pub fn alloc_at_least(count: usize) -> Self {
        GlobalPageAlloc::GLOBAL
            .alloc_at_least(count)
            .expect("Out of memory")
    }

    /// Acquire the ownership of the folio pointed to by [`pfn`], leaving
    /// [`refcount`] untouched.
    ///
    /// # Panic
    /// This function will panic if the folio is not within the global zone.
    ///
    /// # Safety
    /// This function is unsafe because it assumes that the caller has to ensure
    /// that [`pfn`] points to a valid folio allocated through [`Self::alloc()`]
    /// and that the folio have not been freed or deallocated yet.
    pub unsafe fn from_raw(pfn: PFN) -> Self {
        unsafe {
            // SAFETY: The caller ensures that [`pfn`] points to a folio within
            //         the global zone.
            Self(ZONE.get_page(pfn).unwrap_unchecked())
        }
    }

    /// Do some work with the folio without touching the reference count with
    /// the same restrictions as [`Self::from_raw()`].
    ///
    /// # Safety
    /// Check [`Self::from_raw()`] for safety requirements.
    pub unsafe fn with_raw<F, O>(pfn: PFN, func: F) -> O
    where
        F: FnOnce(&Self) -> O,
    {
        unsafe {
            let me = ManuallyDrop::new(Self::from_raw(pfn));
            func(&me)
        }
    }

    pub fn lock(&self) -> LockedFolio {
        // TODO: actually perform the lock...
        LockedFolio(self)
    }

    /// Get a vmem pointer to the folio data as a byte slice.
    pub fn get_bytes_ptr(&self) -> NonNull<[u8]> {
        unsafe {
            // SAFETY: `self.start()` can't be null.
            NonNull::slice_from_raw_parts(self.start().as_ptr(), self.len())
        }
    }

    /// Get a vmem pointer to the start of the folio.
    pub fn get_ptr(&self) -> NonNull<u8> {
        self.get_bytes_ptr().cast()
    }
}

impl Deref for Folio {
    type Target = RawPage;

    fn deref(&self) -> &Self::Target {
        unsafe {
            // SAFETY: We don't expose mutable references to the folio.
            self.0.as_ref()
        }
    }
}

impl Clone for Folio {
    fn clone(&self) -> Self {
        // SAFETY: Memory order here can be Relaxed is for the same reason as
        //         that in the copy constructor of `std::shared_ptr`.
        self.refcount.fetch_add(1, Ordering::Relaxed);

        Self(self.0)
    }
}

impl Drop for Folio {
    fn drop(&mut self) {
        match self.refcount.fetch_sub(1, Ordering::AcqRel) {
            0 => unreachable!("Refcount for an in-use page is 0"),
            1 => unsafe { GlobalPageAlloc::GLOBAL.dealloc_raw(self.0.as_mut()) },
            _ => {}
        }
    }
}

impl fmt::Debug for Folio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Page({:?}, order={})", self.pfn(), self.order)
    }
}

impl FolioTrait for Folio {
    fn pfn(&self) -> PFN {
        ZONE.get_pfn(self.0.as_ptr())
    }

    fn order(&self) -> u32 {
        self.order
    }
}

impl LockedFolio<'_> {
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            // SAFETY: `self.start()` points to valid memory of length `self.len()`.
            core::slice::from_raw_parts(self.start().as_ptr().as_ptr(), self.len())
        }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe {
            // SAFETY: `self.start()` points to valid memory of length `self.len()`.
            core::slice::from_raw_parts_mut(self.start().as_ptr().as_ptr(), self.len())
        }
    }
}

impl Deref for LockedFolio<'_> {
    type Target = Folio;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl FolioOwned {
    pub fn alloc() -> Self {
        Self(Folio::alloc())
    }

    pub fn alloc_order(order: u32) -> Self {
        Self(Folio::alloc_order(order))
    }

    pub fn alloc_at_least(count: usize) -> Self {
        Self(Folio::alloc_at_least(count))
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            // SAFETY: The page is exclusively owned by us.
            self.get_bytes_ptr().as_ref()
        }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe {
            // SAFETY: The page is exclusively owned by us.
            self.get_bytes_ptr().as_mut()
        }
    }

    pub fn share(self) -> Folio {
        self.0
    }
}

impl Deref for FolioOwned {
    type Target = Folio;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
