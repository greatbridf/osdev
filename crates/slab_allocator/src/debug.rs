use core::mem::MaybeUninit;

use eonix_mm::paging::FolioList;

use crate::{SlabAlloc, SlabList, SlabPageAlloc};

pub struct SlabListStat {
    pub object_size: usize,

    pub total_slabs: usize,
    pub active_slabs: usize,
    pub total_objects: usize,
    pub active_objects: usize,
}

impl<T> SlabList<T>
where
    T: FolioList,
{
    pub fn dump_stats(&self) -> SlabListStat {
        SlabListStat {
            object_size: self.object_size,
            total_slabs: self.total_folios,
            active_slabs: self.active_folios,
            total_objects: self.total_objects,
            active_objects: self.active_objects,
        }
    }
}

impl<P, const COUNT: usize> SlabAlloc<P, COUNT>
where
    P: SlabPageAlloc,
{
    pub fn dump_stats(&self) -> [SlabListStat; COUNT] {
        let mut stats: MaybeUninit<[SlabListStat; COUNT]> =
            MaybeUninit::uninit();

        for (i, slab_list) in self.slabs.iter().enumerate() {
            let ptr: *mut SlabListStat = stats.as_mut_ptr().cast();

            unsafe {
                ptr.wrapping_add(i).write(slab_list.lock().dump_stats());
            }
        }

        unsafe {
            // SAFETY: All `COUNT` items are initialized.
            stats.assume_init()
        }
    }
}
