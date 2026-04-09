use eonix_macros::TransparentDeref;
use eonix_mm::paging::PFN;

use crate::kernel::mem::mm_list::add_mapping;
use crate::kernel::mem::{Folio, FolioOwned};

#[repr(transparent)]
#[derive(TransparentDeref)]
pub struct AnonFolio(Folio);

impl AnonFolio {
    pub fn new(folio: FolioOwned) -> Self {
        Self(folio.share())
    }

    pub fn add_mapping(self) -> PFN {
        add_mapping(self.0)
    }
}
