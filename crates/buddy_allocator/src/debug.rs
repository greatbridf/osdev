use eonix_mm::paging::{FolioList, Zone};

use crate::{BuddyAllocator, BuddyFolio, AREAS};

#[derive(Clone, Copy)]
pub struct BuddyStat {
    pub alloced_count: usize,
    pub free_count: usize,
    pub order: u32,
}

impl<Z, L, F> BuddyAllocator<Z, L>
where
    Z: Zone<Page = F>,
    L: FolioList<Folio = F>,
    F: BuddyFolio + 'static,
{
    pub fn dump_stat(&self) -> [BuddyStat; AREAS] {
        let mut stats = [BuddyStat {
            alloced_count: 0,
            free_count: 0,
            order: 0,
        }; AREAS];

        for i in 0..AREAS {
            let area = &self.free_areas[i];
            let stat = &mut stats[i];

            stat.alloced_count = area.alloced;
            stat.free_count = area.count;
            stat.order = i as u32;
        }

        stats
    }
}
