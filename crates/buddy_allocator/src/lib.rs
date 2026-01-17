#![no_std]

use core::hint::unreachable_unchecked;

use eonix_mm::address::{AddrOps as _, PAddr, PRange};
use eonix_mm::paging::{FolioList, FolioListSized, Zone, PFN};

const MAX_ORDER: u32 = 10;
const AREAS: usize = const { MAX_ORDER as usize + 1 };

pub trait BuddyFolio: Sized + 'static {
    fn pfn(&self) -> PFN;

    fn get_order(&self) -> u32;
    fn is_buddy(&self) -> bool;

    fn set_order(&mut self, order: u32);
    fn set_buddy(&mut self, value: bool);
}

struct FreeArea<L>
where
    L: FolioList,
{
    free_list: L,
    count: usize,
}

unsafe impl<L> Send for FreeArea<L> where L: FolioList {}
unsafe impl<L> Sync for FreeArea<L> where L: FolioList {}

pub struct BuddyAllocator<Z, L>
where
    Z: Zone + 'static,
    L: FolioList,
{
    zone: &'static Z,
    free_areas: [FreeArea<L>; AREAS],
}

impl<Z, L> BuddyAllocator<Z, L>
where
    Z: Zone + 'static,
    Z::Page: BuddyFolio,
    L: FolioListSized,
{
    pub const fn new(zone: &'static Z) -> Self {
        Self {
            zone,
            free_areas: [const { FreeArea::new() }; AREAS],
        }
    }
}

impl<Z, L, F> BuddyAllocator<Z, L>
where
    Z: Zone<Page = F>,
    L: FolioList<Folio = F>,
    F: BuddyFolio + 'static,
{
    pub fn create_folios(&mut self, start: PAddr, end: PAddr) {
        assert!(
            self.zone
                .contains_prange(PRange::new(start.ceil(), end.floor())),
            "The given address range is not within the zone."
        );

        let mut pfn = PFN::from(start.ceil());
        let end_pfn = PFN::from(end.floor());

        while pfn < end_pfn {
            let mut order = usize::from(pfn).trailing_zeros().min(MAX_ORDER);
            let new_end_pfn = loop {
                let new_end = pfn + (1 << order);

                if new_end <= end_pfn {
                    break new_end;
                }

                order -= 1;
            };

            unsafe {
                // SAFETY: We've checked that the range is within the zone above.
                self.add_folio_unchecked(pfn, order)
            };

            pfn = new_end_pfn;
        }
    }

    fn add_folio(&mut self, pfn: PFN, order: u32) {
        let prange = PRange::from(PAddr::from(pfn)).grow(1 << (order + 12));
        assert!(
            self.zone.contains_prange(prange),
            "The given folio is not within the zone."
        );

        unsafe {
            // SAFETY: Checks above.
            self.add_folio_unchecked(pfn, order);
        }
    }

    unsafe fn add_folio_unchecked(&mut self, pfn: PFN, order: u32) {
        let Some(mut folio) = self.zone.get_page(pfn) else {
            unsafe { unreachable_unchecked() }
        };

        unsafe {
            // SAFETY: The caller ensures that the page is unused.
            let folio_mut = folio.as_mut();
            self.free_areas[order as usize].add_folio(folio_mut, order);
        }
    }

    fn break_folio(&mut self, folio: &mut F, order: u32, target_order: u32) {
        let pfn = folio.pfn();

        for order in (target_order..order).rev() {
            let buddy_pfn = pfn + (1 << order);

            unsafe {
                // SAFETY: We got the page from `self.free_areas`. Checks are
                //         done when we've put the page into the buddy system.
                self.add_folio_unchecked(buddy_pfn, order);
            }
        }

        folio.set_order(target_order);
    }

    pub fn alloc_order(&mut self, order: u32) -> Option<&'static mut Z::Page> {
        for current_order in order..AREAS as u32 {
            let Some(folio) = self.free_areas[current_order as usize].get_free_folio() else {
                continue;
            };

            if current_order > order {
                self.break_folio(folio, current_order, order);
            }

            return Some(folio);
        }

        None
    }

    pub unsafe fn dealloc(&mut self, folio: &'static mut Z::Page) {
        let mut pfn = folio.pfn();
        let mut order = folio.get_order();

        assert!(
            !folio.is_buddy(),
            "Trying to free a folio that is already in the buddy system: {pfn:?}",
        );

        while order < MAX_ORDER {
            let buddy_pfn = pfn.buddy_pfn(order);
            let Some(buddy) = self.try_get_buddy(buddy_pfn, order) else {
                break;
            };

            self.free_areas[order as usize].remove_folio(buddy);
            pfn = pfn.combined_pfn(buddy_pfn);
            order += 1;
        }

        self.add_folio(pfn, order);
    }

    /// This function checks whether the given page is within our [`Zone`] and
    /// is a free buddy page with the specified order.
    ///
    /// We can assure exclusive access to a buddy page of [`order`] if
    /// - the buddy is within the same [`Zone`] as us.
    /// - the buddy is a free buddy (in some [`FreeArea`])
    /// - the buddy has order [`order`]
    fn try_get_buddy<'a>(&mut self, buddy_pfn: PFN, order: u32) -> Option<&'a mut F> {
        let mut buddy = self.zone.get_page(buddy_pfn)?;

        unsafe {
            // SAFETY: We just test whether the page is a buddy.
            let buddy_ref = buddy.as_ref();

            if !buddy_ref.is_buddy() {
                return None;
            }

            // Sad...
            if buddy_ref.get_order() != order {
                return None;
            }

            // SAFETY: We have the mutable reference to the buddy allocator.
            //         So all the pages within are exclusively accessible to us.
            Some(buddy.as_mut())
        }
    }
}

impl<L> FreeArea<L>
where
    L: FolioListSized,
{
    const fn new() -> Self {
        Self {
            free_list: L::NEW,
            count: 0,
        }
    }
}

impl<L> FreeArea<L>
where
    L: FolioList,
    L::Folio: BuddyFolio + 'static,
{
    pub fn get_free_folio(&mut self) -> Option<&'static mut L::Folio> {
        self.free_list.pop_head().map(|folio| {
            assert_ne!(self.count, 0, "Oops");

            folio.set_buddy(false);
            self.count -= 1;

            folio
        })
    }

    pub fn add_folio(&mut self, folio: &'static mut L::Folio, order: u32) {
        folio.set_order(order);
        folio.set_buddy(true);

        self.count += 1;
        self.free_list.push_tail(folio);
    }

    pub fn remove_folio(&mut self, folio: &mut L::Folio) {
        assert_ne!(self.count, 0, "Oops");
        folio.set_buddy(false);

        self.count -= 1;
        self.free_list.remove(folio);
    }
}

trait BuddyPFNOps {
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
