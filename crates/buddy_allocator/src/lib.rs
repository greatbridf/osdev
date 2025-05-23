#![no_std]

mod free_area;
mod zone;

use core::sync::atomic::Ordering;
use eonix_mm::{
    address::PAddr,
    paging::{RawPage, PFN},
};
use intrusive_list::Link;
use zone::Zone;

const MAX_ORDER: u32 = 10;
const ZONE_AREAS: usize = const { MAX_ORDER as usize + 1 };

pub trait BuddyRawPage: RawPage {
    /// Get the container raw page struct of the list link.
    ///
    /// # Safety
    /// The caller MUST ensure that the link points to a `RawPage`.
    unsafe fn from_link(link: &mut Link) -> Self;

    /// Get the list link of the raw page.
    ///
    /// # Safety
    /// The caller MUST ensure that at any time, only one mutable reference
    /// to the link exists.
    unsafe fn get_link(&self) -> &mut Link;

    fn set_order(&self, order: u32);

    fn is_buddy(&self) -> bool;
    fn is_free(&self) -> bool;

    fn set_buddy(&self);
    fn set_free(&self);

    fn clear_buddy(&self);
    fn clear_free(&self);
}

pub struct BuddyAllocator<T>
where
    T: BuddyRawPage,
{
    zone: Zone<T, ZONE_AREAS>,
}

impl<T> BuddyAllocator<T>
where
    T: BuddyRawPage,
{
    pub const fn new() -> Self {
        Self { zone: Zone::new() }
    }

    pub fn create_pages(&mut self, start: PAddr, end: PAddr) {
        self.zone.create_pages(start, end);
    }

    pub fn alloc_order(&mut self, order: u32) -> Option<T> {
        let pages_ptr = self.zone.get_free_pages(order);

        if let Some(pages_ptr) = pages_ptr {
            // SAFETY: Memory order here can be Relaxed is for the same reason as that
            // in the copy constructor of `std::shared_ptr`.
            pages_ptr.refcount().fetch_add(1, Ordering::Relaxed);
            pages_ptr.clear_free();
        }

        pages_ptr
    }

    pub unsafe fn dealloc(&mut self, page_ptr: T) {
        self.zone.free_pages(page_ptr);
    }

    pub fn has_management_over(page_ptr: T) -> bool {
        !page_ptr.is_free() && page_ptr.is_buddy()
    }
}

pub(self) trait BuddyPFNOps {
    fn buddy_pfn(self, order: u32) -> PFN;
    fn combined_pfn(self, buddy_pfn: PFN) -> PFN;
}

impl BuddyPFNOps for PFN {
    fn buddy_pfn(self, order: u32) -> PFN {
        PFN::from(usize::from(self) ^ (1 << order))
    }

    fn combined_pfn(self, buddy_pfn: PFN) -> PFN {
        PFN::from(usize::from(self) & usize::from(buddy_pfn))
    }
}
