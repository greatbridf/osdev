use super::PFN;
use core::sync::atomic::AtomicUsize;

/// A `RawPage` represents a page of memory in the kernel. It is a low-level
/// representation of a page that is used by the kernel to manage memory.
#[doc(notable_trait)]
pub trait RawPage: Clone + Copy + From<PFN> + Into<PFN> {
    fn order(&self) -> u32;
    fn refcount(&self) -> &AtomicUsize;

    fn is_present(&self) -> bool;
}

#[derive(Clone, Copy)]
pub struct UnmanagedRawPage(PFN, u32);

/// Unmanaged raw pages should always have a non-zero refcount to
/// avoid `free()` from being called.
static UNMANAGED_RAW_PAGE_CLONE_COUNT: AtomicUsize = AtomicUsize::new(1);

impl UnmanagedRawPage {
    pub const fn new(pfn: PFN, order: u32) -> Self {
        Self(pfn, order)
    }
}

impl From<PFN> for UnmanagedRawPage {
    fn from(value: PFN) -> Self {
        Self::new(value, 0)
    }
}

impl Into<PFN> for UnmanagedRawPage {
    fn into(self) -> PFN {
        let Self(pfn, _) = self;
        pfn
    }
}

impl RawPage for UnmanagedRawPage {
    fn order(&self) -> u32 {
        self.1
    }

    fn refcount(&self) -> &AtomicUsize {
        &UNMANAGED_RAW_PAGE_CLONE_COUNT
    }

    fn is_present(&self) -> bool {
        true
    }
}
