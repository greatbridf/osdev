use super::free_area::FreeArea;
use crate::{BuddyPFNOps as _, BuddyRawPage};
use core::sync::atomic::Ordering;
use eonix_mm::{
    address::{AddrOps as _, PAddr},
    paging::PFN,
};

pub(super) struct Zone<T, const AREAS: usize> {
    free_areas: [FreeArea<T>; AREAS],
}

impl<Raw, const AREAS: usize> Zone<Raw, AREAS>
where
    Raw: BuddyRawPage,
{
    pub const fn new() -> Self {
        Self {
            free_areas: [const { FreeArea::new() }; AREAS],
        }
    }

    pub fn get_free_pages(&mut self, order: u32) -> Option<Raw> {
        for current_order in order..AREAS as u32 {
            let pages_ptr = self.free_areas[current_order as usize].get_free_pages();
            let Some(pages_ptr) = pages_ptr else { continue };

            pages_ptr.set_order(order);

            if current_order > order {
                self.expand(pages_ptr, current_order, order);
            }

            assert!(
                pages_ptr.is_present(),
                "Page {:?} is not present",
                pages_ptr.into(),
            );

            assert!(
                pages_ptr.is_free(),
                "Page {:?} is not free",
                pages_ptr.into(),
            );

            return Some(pages_ptr);
        }
        None
    }

    fn expand(&mut self, pages_ptr: Raw, order: u32, target_order: u32) {
        let mut offset = 1 << order;
        let pages_pfn = Into::<PFN>::into(pages_ptr);

        for order in (target_order..order).rev() {
            offset >>= 1;

            let split_pages_ptr = Raw::from(pages_pfn + offset);
            split_pages_ptr.set_order(order);
            split_pages_ptr.set_buddy();
            self.free_areas[order as usize].add_pages(split_pages_ptr);
        }
    }

    pub fn free_pages(&mut self, mut pages_ptr: Raw) {
        assert_eq!(pages_ptr.refcount().load(Ordering::Relaxed), 0);

        let mut pfn = Into::<PFN>::into(pages_ptr);
        let mut current_order = pages_ptr.order();

        assert!(
            pages_ptr.is_present(),
            "Freeing a page that is not present: {:?}",
            pages_ptr.into(),
        );

        assert!(
            !pages_ptr.is_free(),
            "Freeing a page that is free: {:?}",
            pages_ptr.into(),
        );

        while current_order < (AREAS - 1) as u32 {
            let buddy_pfn = pfn.buddy_pfn(current_order);
            let buddy_pages_ptr = Raw::from(buddy_pfn);

            if !self.buddy_check(buddy_pages_ptr, current_order) {
                break;
            }

            pages_ptr.clear_buddy();
            buddy_pages_ptr.clear_buddy();
            self.free_areas[current_order as usize].del_pages(buddy_pages_ptr);

            pages_ptr = Raw::from(pfn.combined_pfn(buddy_pfn));
            pfn = pfn.combined_pfn(buddy_pfn);

            pages_ptr.set_buddy();
            current_order += 1;
        }

        pages_ptr.set_order(current_order);
        self.free_areas[current_order as usize].add_pages(pages_ptr);
    }

    /// This function checks whether a page is free && is a buddy
    /// we can coalesce a page and its buddy if
    /// - the buddy is valid(present) &&
    /// - the buddy is right now in free_areas &&
    /// - a page and its buddy have the same order &&
    /// - a page and its buddy are in the same zone (on smp systems).
    fn buddy_check(&self, pages_ptr: Raw, order: u32) -> bool {
        if !pages_ptr.is_present() {
            return false;
        }
        if !pages_ptr.is_free() {
            return false;
        }
        if pages_ptr.order() != order {
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

            while start_pfn + (1 << order) as usize > end_pfn {
                order -= 1;
            }
            let page_ptr = Raw::from(start_pfn);
            page_ptr.set_buddy();
            self.free_areas[order as usize].add_pages(page_ptr);
            start_pfn = start_pfn + (1 << order) as usize;
        }
    }
}
