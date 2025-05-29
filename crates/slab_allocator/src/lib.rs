#![no_std]

mod slab_cache;

use core::cmp::max;

use eonix_mm::paging::{PageAlloc, RawPage};
use eonix_sync::Spin;
use intrusive_list::Link;
use slab_cache::SlabCache;

pub trait SlabRawPage: RawPage {
    /// Get the container raw page struct of the list link.
    ///
    /// # Safety
    /// The caller MUST ensure that the link points to a `RawPage`.
    unsafe fn from_link(link: &mut Link) -> Self;

    /// Get the list link of the raw page.
    ///
    /// # Safety
    /// The caller MUST ensure that at any time, only one mutable reference
    /// to the link exists.
    unsafe fn get_link(&self) -> &mut Link;

    fn slab_init(&self, object_size: u32);

    fn is_emtpy(&self) -> bool;

    fn is_full(&self) -> bool;

    // which slab page the ptr belong
    fn in_which(ptr: *mut u8) -> Self;

    fn alloc_slot(&self) -> *mut u8;

    fn dealloc_slot(&self, ptr: *mut u8);
}

pub struct SlabAllocator<T, A, const SLAB_CACHE_COUNT: usize> {
    slabs: [Spin<SlabCache<T, A>>; SLAB_CACHE_COUNT],
}

unsafe impl<T, A, const SLAB_CACHE_COUNT: usize> Send for SlabAllocator<T, A, SLAB_CACHE_COUNT> {}
unsafe impl<T, A, const SLAB_CACHE_COUNT: usize> Sync for SlabAllocator<T, A, SLAB_CACHE_COUNT> {}

impl<Raw, Allocator, const SLAB_CACHE_COUNT: usize> SlabAllocator<Raw, Allocator, SLAB_CACHE_COUNT>
where
    Raw: SlabRawPage,
    Allocator: PageAlloc<RawPage = Raw> + Clone,
{
    pub fn new_in(alloc: Allocator) -> Self {
        Self {
            slabs: core::array::from_fn(|i| {
                Spin::new(SlabCache::new_in(1 << (i + 3), alloc.clone()))
            }),
        }
    }

    pub fn alloc(&self, mut size: usize) -> *mut u8 {
        size = max(8, size);
        let idx = size.next_power_of_two().trailing_zeros() - 3;
        self.slabs[idx as usize].lock().alloc()
    }

    pub fn dealloc(&self, ptr: *mut u8, mut size: usize) {
        size = max(8, size);
        let idx = size.next_power_of_two().trailing_zeros() - 3;
        self.slabs[idx as usize].lock().dealloc(ptr);
    }
}
