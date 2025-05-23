use super::PFN;
use core::sync::atomic::AtomicUsize;

/// A `RawPage` represents a page of memory in the kernel. It is a low-level
/// representation of a page that is used by the kernel to manage memory.
pub trait RawPage: Clone + Copy + From<PFN> + Into<PFN> {
    fn order(&self) -> u32;
    fn refcount(&self) -> &AtomicUsize;

    fn is_present(&self) -> bool;
}
