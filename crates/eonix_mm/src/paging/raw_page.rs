use super::PFN;
use core::sync::atomic::AtomicUsize;

/// A `RawPage` represents a page of memory in the kernel. It is a low-level
/// representation of a page that is used by the kernel to manage memory.
pub trait RawPage: Clone + Copy + From<PFN> + Into<PFN> {
    fn order(&self) -> u32;
    fn refcount(&self) -> &AtomicUsize;

    fn is_present(&self) -> bool;
}

#[derive(Clone, Copy)]
pub struct UnmanagedRawPage(PFN);

/// Unmanaged raw pages should always have a non-zero refcount to
/// avoid `free()` from being called.
static UNMANAGED_RAW_PAGE_CLONE_COUNT: AtomicUsize = AtomicUsize::new(1);

impl UnmanagedRawPage {
    pub const fn new(pfn: PFN) -> Self {
        Self(pfn)
    }
}

impl From<PFN> for UnmanagedRawPage {
    fn from(value: PFN) -> Self {
        Self::new(value)
    }
}

impl Into<PFN> for UnmanagedRawPage {
    fn into(self) -> PFN {
        let Self(pfn) = self;
        pfn
    }
}

impl RawPage for UnmanagedRawPage {
    fn order(&self) -> u32 {
        0
    }

    fn refcount(&self) -> &AtomicUsize {
        &UNMANAGED_RAW_PAGE_CLONE_COUNT
    }

    fn is_present(&self) -> bool {
        true
    }
}
