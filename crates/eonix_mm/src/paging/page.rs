use core::mem::ManuallyDrop;
use core::ptr::NonNull;

use super::PFN;
use crate::address::{PAddr, PRange};

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
pub trait PageAccess: Clone {
    /// Returns a kernel-accessible pointer to the page referenced by the given
    /// physical frame number.
    ///
    /// # Safety
    /// This function is unsafe because calling this function on some non-existing
    /// pfn will cause undefined behavior.
    unsafe fn get_ptr_for_pfn(&self, pfn: PFN) -> NonNull<PageBlock>;

    /// Returns a kernel-accessible pointer to the given page.
    fn get_ptr_for_page<F: Folio>(&self, page: &F) -> NonNull<PageBlock> {
        unsafe {
            // SAFETY: `page.pfn()` is guaranteed to be valid.
            self.get_ptr_for_pfn(page.pfn())
        }
    }
}

/// A [`Folio`] represents one page or a bunch of adjacent pages.
pub trait Folio {
    /// Returns the physical frame number of the folio, which is aligned with
    /// the folio's size and valid.
    fn pfn(&self) -> PFN;

    /// Returns the folio's *order* (log2 of the number of pages contained in
    /// the folio).
    fn order(&self) -> u32;

    /// Returns the total size of the folio in bytes.
    fn len(&self) -> usize {
        1 << (self.order() + PAGE_SIZE_BITS)
    }

    /// Returns the start physical address of the folio, which is guaranteed to
    /// be aligned to the folio's size and valid.
    fn start(&self) -> PAddr {
        PAddr::from(self.pfn())
    }

    /// Returns the physical address range of the ifolio, which is guaranteed to
    /// be aligned to the folio's size and valid.
    fn range(&self) -> PRange {
        PRange::from(self.start()).grow(self.len())
    }

    /// Consumes the folio and returns the PFN without dropping the reference
    /// count the folio holds.
    fn into_raw(self) -> PFN
    where
        Self: Sized,
    {
        let me = ManuallyDrop::new(self);
        me.pfn()
    }
}

/// A simple [`Folio`] with no reference counting or other ownership mechanism.
#[derive(Clone)]
pub struct BasicFolio {
    pfn: PFN,
    order: u32,
}

impl BasicFolio {
    pub const fn new(pfn: PFN, order: u32) -> Self {
        Self { pfn, order }
    }
}

impl Folio for BasicFolio {
    fn pfn(&self) -> PFN {
        self.pfn
    }

    fn order(&self) -> u32 {
        self.order
    }
}
