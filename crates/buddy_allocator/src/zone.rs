use crate::BuddyPFNOps as _;

use super::free_area::FreeArea;
use core::sync::atomic::Ordering;
use eonix_mm::{
    address::{AddrOps as _, PAddr},
    paging::{PageFlags, RawPagePtr, PFN},
};

pub(super) struct Zone<const AREAS: usize> {
    free_areas: [FreeArea; AREAS],
}

impl<const AREAS: usize> Zone<AREAS> {
    pub const fn new() -> Self {
        Self {
            free_areas: [const { FreeArea::new() }; AREAS],
        }
    }

    pub fn get_free_pages(&mut self, order: u32) -> Option<RawPagePtr> {
        for current_order in order..AREAS as u32 {
            let pages_ptr = self.free_areas[current_order as usize].get_free_pages();
            let Some(pages_ptr) = pages_ptr else { continue };

            pages_ptr.as_mut().order = order;

            if current_order > order {
                self.expand(pages_ptr, current_order, order);
            }
            assert!(pages_ptr.flags().has(PageFlags::PRESENT | PageFlags::FREE));

            return Some(pages_ptr);
        }
        None
    }

    fn expand(&mut self, pages_ptr: RawPagePtr, order: u32, target_order: u32) {
        let mut offset = 1 << order;

        for order in (target_order..order).rev() {
            offset >>= 1;
            let split_pages_ptr = pages_ptr.offset(offset);
            split_pages_ptr.as_mut().order = order;
            split_pages_ptr.flags().set(PageFlags::BUDDY);
            self.free_areas[order as usize].add_pages(split_pages_ptr);
        }
    }

    pub fn free_pages(&mut self, mut pages_ptr: RawPagePtr) {
        assert_eq!(pages_ptr.refcount().load(Ordering::Relaxed), 0);

        let mut pfn = PFN::from(pages_ptr);
        let mut current_order = pages_ptr.order();

        while current_order < (AREAS - 1) as u32 {
            let buddy_pfn = pfn.buddy_pfn(current_order);
            let buddy_pages_ptr = RawPagePtr::from(buddy_pfn);

            if !self.buddy_check(buddy_pages_ptr, current_order) {
                break;
            }

            pages_ptr.flags().clear(PageFlags::BUDDY);
            buddy_pages_ptr.flags().clear(PageFlags::BUDDY);
            self.free_areas[current_order as usize].del_pages(buddy_pages_ptr);

            pages_ptr = RawPagePtr::from(pfn.combined_pfn(buddy_pfn));
            pfn = pfn.combined_pfn(buddy_pfn);

            pages_ptr.flags().set(PageFlags::BUDDY);
            current_order += 1;
        }

        pages_ptr.as_mut().order = current_order;
        self.free_areas[current_order as usize].add_pages(pages_ptr);
    }

    /// This function checks whether a page is free && is a buddy
    /// we can coalesce a page and its buddy if
    /// - the buddy is valid(present) &&
    /// - the buddy is right now in free_areas &&
    /// - a page and its buddy have the same order &&
    /// - a page and its buddy are in the same zone.    // check when smp
    fn buddy_check(&self, pages_ptr: RawPagePtr, order: u32) -> bool {
        if !pages_ptr.flags().has(PageFlags::PRESENT) {
            return false;
        }
        if !pages_ptr.flags().has(PageFlags::FREE) {
            return false;
        }
        if pages_ptr.flags().has(PageFlags::LOCAL) {
            return false;
        }
        if pages_ptr.as_ref().order != order {
            return false;
        }

        assert_eq!(pages_ptr.refcount().load(Ordering::Relaxed), 0);
        true
    }

    /// Only used on buddy initialization
    pub fn create_pages(&mut self, start: PAddr, end: PAddr) {
        let mut start_pfn = PFN::from(start.ceil());
        let end_pfn = PFN::from(end.floor());

        while start_pfn < end_pfn {
            let mut order = usize::from(start_pfn)
                .trailing_zeros()
                .min((AREAS - 1) as u32);

            while start_pfn + order as usize > end_pfn {
                order -= 1;
            }
            let page_ptr: RawPagePtr = start_pfn.into();
            page_ptr.flags().set(PageFlags::BUDDY);
            self.free_areas[order as usize].add_pages(page_ptr);
            start_pfn = start_pfn + (1 << order) as usize;
        }
    }
}
