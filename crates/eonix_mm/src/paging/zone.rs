use core::cell::UnsafeCell;

#[allow(unused_imports)]
use super::{Page, PageAlloc, RawPage, PFN};
use crate::address::PRange;

/// A [`Zone`] holds a lot of [`Page`]s that share the same NUMA node or
/// "physical location".
pub trait Zone: Send + Sync {
    type Page;

    /// Whether the [`range`] is within this [`Zone`].
    fn contains_prange(&self, range: PRange) -> bool;

    /// Get the [`RawPage`] that [`pfn`] points to.
    ///
    /// # Return
    /// [`None`] if [`pfn`] is not in this [`Zone`].
    fn get_page(&self, pfn: PFN) -> Option<&UnsafeCell<Self::Page>>;
}
