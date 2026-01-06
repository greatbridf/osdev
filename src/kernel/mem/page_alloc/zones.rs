use core::cell::UnsafeCell;

use eonix_mm::address::PRange;
use eonix_mm::paging::{Zone, PFN};

use super::RawPage;
use crate::kernel::mem::page_alloc::RawPagePtr;

pub struct GlobalZone();

impl Zone for GlobalZone {
    type Page = RawPage;

    fn contains_prange(&self, _: PRange) -> bool {
        true
    }

    fn get_page(&self, pfn: PFN) -> Option<&UnsafeCell<Self::Page>> {
        unsafe {
            // SAFETY: The pointer returned by [`RawPagePtr::as_ptr()`] is valid.
            //         And so is it wrapped with [`UnsafeCell`]
            Some(&*(RawPagePtr::from(pfn).as_ptr() as *const UnsafeCell<Self::Page>))
        }
    }
}
