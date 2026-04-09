use eonix_macros::TransparentDeref;
use eonix_mm::paging::PFN;

use crate::kernel::mem::{FolioOwned, MapFolio};

#[repr(transparent)]
#[derive(TransparentDeref)]
pub struct AnonFolio(MapFolio);

impl AnonFolio {
    pub fn new(folio: FolioOwned) -> Self {
        Self(folio.into_mappable())
    }

    pub fn add_mapping(self) -> PFN {
        let Self(map_folio) = self;
        map_folio.add_mapping()
    }
}
