use core::cell::UnsafeCell;

use eonix_macros::TransparentDeref;
use eonix_sync::atomic;

use crate::kernel::mem::mapped::MapFolioData;
use crate::kernel::mem::page_alloc::PageFlags;
use crate::kernel::mem::{Folio, RawPage};

pub struct MapRawPageData(UnsafeCell<MapFolioData>);

/// # Invariant
/// A MapPage always has MAPPED bit set and has initialized MapPageData.
#[repr(transparent)]
#[derive(TransparentDeref)]
struct MapRawPage(RawPage);

impl MapRawPage {
    fn data(&self) -> &MapFolioData {
        unsafe {
            // SAFETY: Guaranteed by the type invariant. Sync with writers by
            //         the atomic operations of MAPPED bit.
            &*self.shared_data.map.0.get()
        }
    }
}

impl Folio {
    /// Get the folio's map data without synchronization overhead.
    ///
    /// # Safety
    /// The folio must have been previously initialized as mappable through
    /// [`Self::_make_map`] and synchronized with the one initializing it.
    pub unsafe fn _map_data_unchecked(&self) -> &MapFolioData {
        let raw_page: &RawPage = self;

        let map_raw_page = unsafe {
            // SAFETY: `MapPage` is `repr(transparent)`.
            &*(raw_page as *const RawPage as *const MapRawPage)
        };

        map_raw_page.data()
    }

    /// Get the folio's map data.
    pub fn _map_data(&self) -> &MapFolioData {
        assert!(self.is_map(), "Not mappable: {:?}", self);

        unsafe {
            // SAFETY: Synchronized using acquire semantics.
            self._map_data_unchecked()
        }
    }

    /// Check whether the folio is intialized as mappable with acquire semantics.
    pub fn is_map(&self) -> bool {
        atomic!(@Acquire, self.flags, has, PageFlags::MAPPED)
    }

    /// Make the folio a mappable one.
    ///
    /// # Safety
    /// The caller must exclude concurrent calls.
    /// Otherwise, it is an undefined behavior.
    pub unsafe fn _make_map(folio: &Self) {
        debug_assert!(
            !atomic!(@Relaxed, folio.flags, has,
                    PageFlags::SLAB | PageFlags::MAPPED),
            "Conflict flags"
        );

        unsafe {
            // SAFETY: We have the page lock held to exclude concurrent
            //         make_mapping callers. And readers sync with us through
            //         acquiring the MAPPED bit.
            folio.shared_data.map.0.get().write(MapFolioData::new());
        }

        atomic!(@Release, folio.flags, set, PageFlags::MAPPED);
    }

    /// Make the folio not mappable.
    ///
    /// # Safety
    /// The caller must exclude concurrent calls and must not use it as mappable
    /// after calling this function.
    ///
    /// Otherwise, it is an undefined behavior.
    pub unsafe fn _drop_map(folio: &mut Self) {
        let _map_data = unsafe {
            // SAFETY: We are dropping the mapping data this folio holds.
            folio.shared_data.map.0.get().read()
        };

        atomic!(@Release, folio.flags, clear, PageFlags::MAPPED);
    }
}
