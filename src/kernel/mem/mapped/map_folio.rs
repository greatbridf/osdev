use core::sync::atomic::AtomicUsize;

use eonix_macros::TransparentDeref;
use eonix_mm::paging::{Folio as _, PFN};
use eonix_sync::atomic;

use crate::kernel::mem::{Folio, FolioOwned};

pub struct MapFolioData {
    mapcount: AtomicUsize,
}

#[repr(transparent)]
#[derive(Clone, TransparentDeref)]
pub struct MapFolio(Folio);

impl MapFolioData {
    pub const fn new() -> Self {
        Self {
            mapcount: AtomicUsize::new(0),
        }
    }
}

impl FolioOwned {
    /// Make the folio a mappable one.
    pub fn into_mappable(self) -> MapFolio {
        assert!(!self.is_map(), "Already mappable: {:?}", &self as &Folio);

        unsafe {
            // SAFETY: We own the folio.
            Folio::_make_map(&self);
        }

        MapFolio(self.share())
    }
}

impl MapFolio {
    /// Borrow the mappable folio from page table.
    ///
    /// # Safety
    /// `pfn` must be previously created via [`Self::add_mapping`] and the
    /// mapping must not be removed during the function call.
    ///
    /// Otherwise this is undefined behavior.
    pub unsafe fn with_mapping<O>(
        pfn: PFN, func: impl FnOnce(&Self) -> O,
    ) -> O {
        unsafe {
            // SAFETY: `add_mapping()` calls `Folio::into_raw()`
            Folio::with_raw(pfn, |folio| {
                // SAFETY: MapFolio is repr(transparent)
                let map_folio = &*(folio as *const Folio as *const MapFolio);

                func(map_folio)
            })
        }
    }

    fn map_data(&self) -> &MapFolioData {
        unsafe {
            // SAFETY: Guaranteed by the invariance.
            self._map_data_unchecked()
        }
    }

    /// Turn the folio together with its refcount into a raw [`PFN`] that can be
    /// mapped into some page table.
    pub fn add_mapping(self) -> PFN {
        let map_data = self.map_data();

        atomic!(@Relaxed, map_data.mapcount, fetch_add, 1);

        unsafe {
            // SAFETY: MapFolio is repr(transparent)
            core::mem::transmute::<Self, Folio>(self).into_raw()
        }
    }

    /// Duplicate the mapping in page tables.
    ///
    /// # Safety
    /// `pfn` must be previously created via [`Self::add_mapping`].
    ///
    /// Otherwise this is undefined behavior.
    pub unsafe fn duplicate_mapping(pfn: PFN) -> PFN {
        unsafe {
            // SAFETY: See above.
            Self::with_mapping(pfn, |map_folio| {
                Self(map_folio.0.clone()).add_mapping()
            })
        }
    }

    /// Remove the mapping from the page table.
    ///
    /// # Safety
    /// `pfn` must be previously created via [`Self::add_mapping`].
    ///
    /// Otherwise this is undefined behavior.
    pub unsafe fn remove_mapping(pfn: PFN) -> Self {
        let folio = unsafe {
            // SAFETY: `pfn` is created via `add_mapping`, which uses `into_raw`.
            Folio::from_raw(pfn)
        };

        let map_folio = Self(folio);
        let map_data = map_folio.map_data();

        atomic!(@AcqRel, map_data.mapcount, fetch_sub, 1);

        map_folio
    }

    pub fn is_exclusive(&self) -> bool {
        let map_data = self.map_data();
        let mapcount = atomic!(@Acquire, map_data.mapcount, load);

        mapcount == 1
    }

    pub fn into_inner(self) -> Folio {
        let Self(folio) = self;

        folio
    }
}
