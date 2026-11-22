use core::fmt;
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::ops::Deref;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;

use eonix_mm::paging::{
    Folio as FolioTrait, FrameAlloc, GlobalFrameAlloc, Zone, PFN,
};

use super::page_alloc::{AnyExt, PageExt, ZONE};
use super::{GlobalPageAlloc, Page, PhysAccess as _};

pub struct Shared;
pub struct Exclusive;
pub struct Locked<'a>(PhantomData<&'a ()>);

pub unsafe trait Ownership {
    const HOLDS_REF: bool;
}
pub unsafe trait NonShared: Ownership {}
pub unsafe trait Owned: Ownership {
    fn share(me: FolioOwned) -> FolioObj<AnyExt, Self>
    where
        Self: Sized;
}

unsafe impl Ownership for Shared {
    const HOLDS_REF: bool = true;
}
unsafe impl Ownership for Exclusive {
    const HOLDS_REF: bool = true;
}
unsafe impl Ownership for Locked<'_> {
    const HOLDS_REF: bool = false;
}

unsafe impl NonShared for Exclusive {}
unsafe impl NonShared for Locked<'_> {}

unsafe impl Owned for Shared {
    fn share(me: FolioOwned) -> FolioObj<AnyExt, Self>
    where
        Self: Sized,
    {
        me.share()
    }
}

unsafe impl Owned for Exclusive {
    fn share(me: FolioOwned) -> FolioObj<AnyExt, Self>
    where
        Self: Sized,
    {
        me
    }
}

#[repr(transparent)]
pub struct FolioObj<E = AnyExt, O = Shared>(NonNull<Page<E>>, PhantomData<O>)
where
    E: PageExt,
    O: Ownership;

pub type Folio = FolioObj;
pub type FolioOwned = FolioObj<AnyExt, Exclusive>;
pub type LockedFolio<'a> = FolioObj<AnyExt, Locked<'a>>;

unsafe impl<E, O> Send for FolioObj<E, O>
where
    E: PageExt,
    O: Ownership,
{
}

unsafe impl<E, O> Sync for FolioObj<E, O>
where
    E: PageExt,
    O: Ownership,
{
}

impl FolioOwned {
    pub(super) const fn from_mut_page(raw_page: &'static mut Page) -> Self {
        Self(NonNull::new(raw_page).unwrap(), PhantomData)
    }
}

impl<O> FolioObj<AnyExt, O>
where
    O: Owned,
{
    /// Allocate a folio of the given *order*.
    pub fn alloc_order(order: u32) -> Self {
        O::share(
            GlobalPageAlloc::GLOBAL
                .alloc_order(order)
                .expect("Out of memory"),
        )
    }

    /// Allocate a folio of order 0
    pub fn alloc() -> Self {
        Self::alloc_order(0)
    }

    /// Allocate a folio consisting of at least [`count`] pages.
    pub fn alloc_at_least(count: usize) -> Self {
        O::share(
            GlobalPageAlloc::GLOBAL
                .alloc_at_least(count)
                .expect("Out of memory"),
        )
    }
}

impl Folio {
    pub fn lock(&self) -> LockedFolio<'_> {
        // TODO: actually perform the lock...
        FolioObj(self.0, PhantomData)
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
            Self(ZONE.get_page(pfn).unwrap_unchecked(), PhantomData)
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

    pub fn unshare(self) -> FolioOwned {
        assert_eq!(self.refcount.load(Ordering::Acquire), 1);
        let me = ManuallyDrop::new(self);
        FolioObj(me.0, PhantomData)
    }
}

impl<N: Ownership> FolioObj<AnyExt, N> {
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

impl<E: PageExt, O: Ownership> Deref for FolioObj<E, O> {
    type Target = Page<E>;

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

        Self(self.0, PhantomData)
    }
}

impl<E: PageExt, O: Ownership> Drop for FolioObj<E, O> {
    fn drop(&mut self) {
        if !O::HOLDS_REF {
            return;
        }

        match self.refcount.fetch_sub(1, Ordering::AcqRel) {
            0 => unreachable!("Refcount for an in-use page is 0"),
            1 => unsafe {
                GlobalPageAlloc::GLOBAL.dealloc_raw(self.0.as_mut().as_mut())
            },
            _ => {}
        }
    }
}

impl<E: PageExt, O: Ownership> fmt::Debug for FolioObj<E, O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Page({:?}, order={})", self.pfn(), self.order)
    }
}

impl<E: PageExt, O: Ownership> FolioTrait for FolioObj<E, O> {
    fn pfn(&self) -> PFN {
        ZONE.get_pfn(self.0.as_ptr() as *const Page)
    }

    fn order(&self) -> u32 {
        self.order
    }
}

impl<O: NonShared> FolioObj<AnyExt, O> {
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            // SAFETY: `self.start()` points to valid memory of length `self.len()`.
            core::slice::from_raw_parts(
                self.start().as_ptr().as_ptr(),
                self.len(),
            )
        }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe {
            // SAFETY: `self.start()` points to valid memory of length `self.len()`.
            core::slice::from_raw_parts_mut(
                self.start().as_ptr().as_ptr(),
                self.len(),
            )
        }
    }
}

impl FolioOwned {
    pub fn share(self) -> Folio {
        let me = ManuallyDrop::new(self);
        FolioObj(me.0, PhantomData)
    }
}
