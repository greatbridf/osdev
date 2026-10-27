use core::ptr::NonNull;

use eonix_mm::address::PRange;
use eonix_mm::paging::{Zone, PFN};

use super::RawPage;

pub static ZONE: GlobalZone = GlobalZone();

const PAGE_ARRAY: NonNull<RawPage> =
    unsafe { NonNull::new_unchecked(0xffffff8040000000 as *mut _) };

pub struct GlobalZone();

impl GlobalZone {
    pub fn get_pfn(&self, page_ptr: *const RawPage) -> PFN {
        PFN::from(unsafe { page_ptr.offset_from(PAGE_ARRAY.as_ptr()) as usize })
    }
}

impl Zone for GlobalZone {
    type Page = RawPage;

    fn contains_prange(&self, _: PRange) -> bool {
        true
    }

    fn get_page(&self, pfn: PFN) -> Option<NonNull<RawPage>> {
        Some(unsafe { PAGE_ARRAY.add(usize::from(pfn)) })
    }
}
