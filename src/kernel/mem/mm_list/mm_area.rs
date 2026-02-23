use alloc::sync::Arc;
use core::cell::UnsafeCell;
use core::cmp;
use core::sync::atomic::Ordering;

use eonix_mm::address::{AddrOps as _, VAddr, VRange};
use eonix_mm::page_table::{PageAttribute, RawAttribute, PTE};
use eonix_mm::paging::{Folio as _, PFN};
use intrusive_collections::rbtree::Entry;
use intrusive_collections::{
    intrusive_adapter, Bound, KeyAdapter, PointerOps, RBTree, RBTreeAtomicLink,
};

use super::{Mapping, EMPTY_PAGE};
use crate::kernel::mem::folio::Folio;
use crate::kernel::mem::mm_list::MMListInner;
use crate::kernel::mem::{CachePage, FolioOwned, PageOffset, Permission};
use crate::prelude::KResult;

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct AreaFlags: u32 {
        const READ = 1 << 0;
        const WRITE = 1 << 1;
        const EXECUTE = 1 << 2;
        const SHARED = 1 << 3;
    }
}

/// # Lock
/// Protected by [`MMListInner`] lock.
pub struct RangeProtected(UnsafeCell<VRange>);

unsafe impl Send for RangeProtected {}
unsafe impl Sync for RangeProtected {}

impl Clone for RangeProtected {
    fn clone(&self) -> Self {
        // TODO: CHANGE THIS
        let range = unsafe { self.as_ref_unchecked().clone() };

        Self(UnsafeCell::new(range))
    }
}

#[derive(Clone)]
pub struct MemArea {
    /// # Lock
    /// Protected by [`MMListInner`] lock.
    pub range: RangeProtected,
    pub flags: AreaFlags,
    areas_link: RBTreeAtomicLink,

    mapping: Mapping,
}

intrusive_adapter!(AreasAdapter = Arc<MemArea>: MemArea { areas_link: RBTreeAtomicLink });

impl<'a> KeyAdapter<'a> for AreasAdapter {
    type Key = VRange;

    fn get_key(
        &self, value: &'a <Self::PointerOps as PointerOps>::Value,
    ) -> Self::Key {
        unsafe {
            // SAFETY: The [`AreaList`] is within the [`MMListInner`].
            //         References to the areas list implies holding the lock of
            //         [`MMListInner`], so it's safe to read the range.
            *value.range.as_ref_unchecked()
        }
    }
}

pub struct AreaList {
    areas: RBTree<AreasAdapter>,
}

impl AreaFlags {
    pub fn from_old(perm: Permission, is_shared: bool) -> Self {
        let mut flags = AreaFlags::empty();

        flags.set(AreaFlags::READ, perm.read);
        flags.set(AreaFlags::WRITE, perm.write);
        flags.set(AreaFlags::EXECUTE, perm.execute);
        flags.set(AreaFlags::SHARED, is_shared);

        flags
    }
}

impl RangeProtected {
    pub const fn new(range: VRange) -> Self {
        Self(UnsafeCell::new(range))
    }

    pub unsafe fn as_ref_unchecked<'a>(&self) -> &'a VRange {
        unsafe { &*self.0.get() }
    }

    pub fn as_ref(&self, _list_read_lock: &MMListInner) -> &VRange {
        unsafe {
            // SAFETY: If we are holding the list's read lock, we won't modify
            // the areas and so won't the range, so it's safe to return a
            // reference.
            self.as_ref_unchecked()
        }
    }

    pub fn as_mut<'a>(
        &self, _list_write_lock: &'a mut AreaList,
    ) -> &'a mut VRange {
        unsafe {
            // SAFETY: If we are holding the list's write lock, we can guarantee
            // that no one else is reading or writing the areas, so it's safe to
            // return a mutable reference.
            &mut *self.0.get()
        }
    }

    pub fn get_mut(&mut self) -> &mut VRange {
        self.0.get_mut()
    }
}

impl AreaList {
    pub const fn new() -> Self {
        Self {
            areas: RBTree::new(AreasAdapter::NEW),
        }
    }

    pub fn insert_new(&mut self, area: Arc<MemArea>) {
        let range = unsafe { area.range.as_ref_unchecked() };
        match self.areas.entry(range) {
            Entry::Occupied(_) => panic!("Overlapping mem area: {range:?}."),
            Entry::Vacant(insert_cursor) => {
                insert_cursor.insert(area);
            }
        }
    }

    pub fn get(&self, addr: VAddr) -> Option<Arc<MemArea>> {
        self.areas.find(&VRange::from(addr)).clone_pointer()
    }

    pub fn contains(&self, addr: VAddr) -> bool {
        !self.areas.find(&VRange::from(addr)).is_null()
    }

    /// Return the mem area with the highest start address that is below or
    /// overlaps with the given address.
    pub fn upper_bound(&self, addr: VAddr) -> Option<Arc<MemArea>> {
        self.areas
            .upper_bound(Bound::Included(&VRange::from(addr)))
            .clone_pointer()
    }

    pub fn contains_range(&self, range: &VRange) -> bool {
        let Some(ub) = self.upper_bound(range.end()) else {
            return false;
        };

        let range = unsafe {
            // SAFETY: Check `KeyAdapter` impl above.
            ub.range.as_ref_unchecked()
        };

        range.end() <= range.start()
    }

    // TODO: For backwards compatibility. Remove this.
    pub fn retain(&mut self, mut pred: impl FnMut(&MemArea) -> bool) {
        let mut cursor = self.areas.front_mut();

        while let Some(area) = cursor.get() {
            if !pred(area) {
                cursor.remove();
            }

            cursor.move_next();
        }
    }

    // TODO: For backwards compatibility. Remove this.
    pub fn insert(&mut self, area: MemArea) {
        let range = unsafe { area.range.as_ref_unchecked() };

        match self.areas.entry(range) {
            Entry::Occupied(_) => panic!("Overlapping mem area: {:?}.", range),
            Entry::Vacant(insert_cursor) => {
                insert_cursor.insert(Arc::new(area));
            }
        }
    }

    // TODO: For backwards compatibility. Remove this.
    pub fn deep_clone(&self) -> Self {
        let mut areas = RBTree::new(AreasAdapter::NEW);

        for area in self.areas.iter() {
            areas.insert(Arc::new(area.clone()));
        }

        Self { areas }
    }

    pub fn iter(&self) -> impl Iterator<Item = &MemArea> {
        let mut cursor = self.areas.front();

        core::iter::from_fn(move || {
            let area = cursor.get()?;
            cursor.move_next();

            Some(area)
        })
    }

    // TODO: For backwards compatibility. Remove this.
    pub fn take(&mut self) -> impl IntoIterator<Item = Arc<MemArea>> {
        self.areas.take()
    }
}

impl MemArea {
    pub const fn new(
        range: VRange, flags: AreaFlags, mapping: Mapping,
    ) -> Self {
        Self {
            range: RangeProtected::new(range),
            flags,
            areas_link: RBTreeAtomicLink::new(),
            mapping,
        }
    }

    pub fn set_permission(&mut self, perm: Permission) {
        self.flags.set(AreaFlags::READ, perm.read);
        self.flags.set(AreaFlags::WRITE, perm.write);
        self.flags.set(AreaFlags::EXECUTE, perm.execute);
    }

    pub fn is_shared(&self) -> bool {
        self.flags.contains(AreaFlags::SHARED)
    }

    pub fn can_read(&self) -> bool {
        self.flags.contains(AreaFlags::READ)
    }

    pub fn can_write(&self) -> bool {
        self.flags.contains(AreaFlags::WRITE)
    }

    pub fn can_execute(&self) -> bool {
        self.flags.contains(AreaFlags::EXECUTE)
    }

    /// # Safety
    /// This function should be called only when we can guarantee that the range
    /// won't overlap with any other range in some scope.
    pub fn grow(&self, count: usize, areas: &mut AreaList) {
        // TODO: Remove this.
        let range = self.range.as_mut(areas);
        *range = range.grow(count);
    }

    pub fn split(mut self, at: VAddr) -> (Option<Self>, Option<Self>) {
        assert!(at.is_page_aligned());
        let range = self.range.get_mut();

        match (*range).cmp(&VRange::from(at)) {
            cmp::Ordering::Less => (Some(self), None),
            cmp::Ordering::Greater => (None, Some(self)),
            cmp::Ordering::Equal => {
                let diff = at - range.start();
                if diff == 0 {
                    return (None, Some(self));
                }

                let right = Self {
                    range: RangeProtected::new(VRange::new(at, range.end())),
                    flags: self.flags.clone(),
                    areas_link: RBTreeAtomicLink::new(),
                    mapping: match &self.mapping {
                        Mapping::Anonymous => Mapping::Anonymous,
                        Mapping::File(mapping) => {
                            Mapping::File(mapping.offset(diff))
                        }
                    },
                };

                let new_range = range.shrink(range.end() - at);

                *self.range.get_mut() = new_range;
                (Some(self), Some(right))
            }
        }
    }

    pub fn handle_cow(&self, pfn: &mut PFN, attr: &mut PageAttribute) {
        assert!(attr.contains(PageAttribute::COPY_ON_WRITE));

        attr.remove(PageAttribute::COPY_ON_WRITE);
        attr.set(PageAttribute::WRITE, self.can_write());

        let page = unsafe { Folio::from_raw(*pfn) };

        // XXX: Change me!!!
        if page.refcount.load(Ordering::Relaxed) == 1 {
            // SAFETY: This is actually safe. If we read `1` here and we have `MMList` lock
            // held, there couldn't be neither other processes sharing the page, nor other
            // threads making the page COW at the same time.
            core::mem::forget(page);
            return;
        }

        let mut new_page;
        if *pfn == EMPTY_PAGE.pfn() {
            new_page = {
                let mut folio = FolioOwned::alloc();
                folio.as_bytes_mut().fill(0);
                folio
            };
        } else {
            new_page = FolioOwned::alloc();

            unsafe {
                // SAFETY: `page` is CoW, which means that others won't write to it.
                let old_page_data = page.get_bytes_ptr().as_ref();
                let new_page_data = new_page.as_bytes_mut();

                new_page_data.copy_from_slice(old_page_data);
            };
        }

        attr.remove(PageAttribute::ACCESSED);
        *pfn = new_page.share().into_raw();
    }

    /// # Arguments
    /// * `offset`: The offset from the start of the mapping, aligned to 4KB boundary.
    pub async fn handle_mmap(
        &self, pfn: &mut PFN, attr: &mut PageAttribute, offset: usize,
        write: bool,
    ) -> KResult<()> {
        let Mapping::File(file_mapping) = &self.mapping else {
            panic!("Anonymous mapping should not be PA_MMAP");
        };

        assert!(offset < file_mapping.length, "Offset out of range");

        let file_offset = file_mapping.offset + offset;

        let map_page = |cache_page: &CachePage| {
            if !self.can_write() {
                assert!(!write, "Write fault on read-only mapping");

                *pfn = cache_page.add_mapping();
                return;
            }

            if self.is_shared() {
                // We don't process dirty flags in write faults.
                // Simply assume that page will eventually be dirtied.
                // So here we can set the dirty flag now.
                cache_page.set_dirty(true);
                attr.insert(PageAttribute::WRITE);
                *pfn = cache_page.add_mapping();
                return;
            }

            if !write {
                // Delay the copy-on-write until write fault happens.
                attr.insert(PageAttribute::COPY_ON_WRITE);
                *pfn = cache_page.add_mapping();
                return;
            }

            // XXX: Change this. Let's handle mapped pages before CoW pages.
            // Nah, we are writing to a mapped private mapping...
            let mut new_page = FolioOwned::alloc();
            new_page
                .as_bytes_mut()
                .copy_from_slice(cache_page.lock().as_bytes());

            attr.insert(PageAttribute::WRITE);
            *pfn = new_page.share().into_raw();
        };

        file_mapping
            .page_cache
            .with_page(PageOffset::from_byte_floor(file_offset), map_page)
            .await?;

        attr.insert(PageAttribute::PRESENT);
        attr.remove(PageAttribute::MAPPED);
        Ok(())
    }

    pub async fn handle(
        &self, pte: &mut impl PTE, offset: usize, write: bool,
    ) -> KResult<()> {
        let mut attr =
            pte.get_attr().as_page_attr().expect("Not a page attribute");
        let mut pfn = pte.get_pfn();

        if attr.contains(PageAttribute::COPY_ON_WRITE) {
            self.handle_cow(&mut pfn, &mut attr);
        }

        if attr.contains(PageAttribute::MAPPED) {
            self.handle_mmap(&mut pfn, &mut attr, offset, write).await?;
        }

        attr.insert(PageAttribute::ACCESSED);

        if write {
            attr.insert(PageAttribute::DIRTY);
        }

        pte.set(pfn, attr.into());

        Ok(())
    }
}
