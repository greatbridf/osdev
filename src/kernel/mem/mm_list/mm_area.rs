use alloc::sync::Arc;
use core::cell::UnsafeCell;
use core::mem::ManuallyDrop;
use core::sync::atomic::{AtomicU32, Ordering};

use eonix_mm::address::{VAddr, VRange};
use eonix_mm::page_table::{PageAttribute, RawAttribute, PTE};
use eonix_mm::paging::{Folio as _, PFN};
use eonix_sync::Mutex;
use intrusive_collections::rbtree::Entry;
use intrusive_collections::{
    intrusive_adapter, Bound, KeyAdapter, LinkedList, LinkedListAtomicLink,
    PointerOps, RBTree, RBTreeAtomicLink,
};

use super::{Mapping, EMPTY_PAGE};
use crate::kernel::mem::folio::Folio;
use crate::kernel::mem::mm_list::MemListLock;
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

struct AtomicFlags(AtomicU32);

mod sealed {
    pub trait RangeReadLock {}
}

/// Some lock that can provide read protection for the area range, including a
/// reference to the MemList, its lock, the area list lock and the area lock.
pub trait RangeReadLock: sealed::RangeReadLock {}

impl RangeReadLock for &MemListLock {}
impl RangeReadLock for &AreaListLock {}
impl RangeReadLock for &AreaLock {}
impl sealed::RangeReadLock for &MemListLock {}
impl sealed::RangeReadLock for &AreaListLock {}
impl sealed::RangeReadLock for &AreaLock {}

/// # Lock
/// Protected by [`MMListInner`] lock.
pub struct RangeProtected(UnsafeCell<VRange>);

unsafe impl Send for RangeProtected {}
unsafe impl Sync for RangeProtected {}

pub struct AreaLock {
    _phantom: (),
}

pub struct AreaListLock {
    _phantom: (),
}

union Link {
    rbtree: ManuallyDrop<RBTreeAtomicLink>,
    list: ManuallyDrop<LinkedListAtomicLink>,
}

pub struct MemArea {
    /// # Lock
    /// Protected by [`MMListInner`] lock.
    pub range: RangeProtected,
    flags: AtomicFlags,
    pub lock: Mutex<AreaLock>,
    link: Link,

    mapping: Mapping,
}

// SAFETY: `areas_link` is larger in size than `LinkedListAtomicLink`.
intrusive_adapter!(ListAdapter = Arc<MemArea>: MemArea { link: LinkedListAtomicLink });
intrusive_adapter!(AreasAdapter = Arc<MemArea>: MemArea { link: RBTreeAtomicLink });

impl<'a> KeyAdapter<'a> for AreasAdapter {
    type Key = VRange;

    fn get_key(
        &self, value: &'a <Self::PointerOps as PointerOps>::Value,
    ) -> Self::Key {
        unsafe {
            // SAFETY: The [`AreaList`] is within the [`MMListInner`].
            //         References to the areas list implies holding the lock of
            //         [`MMListInner`], so it's safe to read the range.
            value.range.as_ref_unchecked().clone()
        }
    }
}

pub struct AreaList {
    areas: RBTree<AreasAdapter>,
    pub lock: AreaListLock,
}

impl AreaLock {
    const fn _new() -> Self {
        Self { _phantom: () }
    }
}

impl AreaFlags {
    const RWX: Self = Self::READ.union(Self::WRITE).union(Self::EXECUTE);

    pub fn from_old(perm: Permission, is_shared: bool) -> Self {
        let mut flags = AreaFlags::empty();

        flags.set(AreaFlags::READ, perm.read);
        flags.set(AreaFlags::WRITE, perm.write);
        flags.set(AreaFlags::EXECUTE, perm.execute);
        flags.set(AreaFlags::SHARED, is_shared);

        flags
    }
}

impl AtomicFlags {
    const fn new(flags: AreaFlags) -> Self {
        Self(AtomicU32::new(flags.bits()))
    }

    /// Load with Acquire semantics.
    fn load(&self) -> AreaFlags {
        AreaFlags::from_bits_retain(self.0.load(Ordering::Acquire))
    }

    /// Do atomic read-modify-write with AcqRel semantics on success and
    /// Relaxed semantics on failure.
    fn update(
        &self, mut try_modify: impl FnMut(AreaFlags) -> AreaFlags,
    ) -> AreaFlags {
        loop {
            let Ok(old_raw) = self.0.fetch_update(
                Ordering::AcqRel,
                Ordering::Relaxed,
                |raw| {
                    let flags = AreaFlags::from_bits_retain(raw);
                    Some(try_modify(flags).bits())
                },
            ) else {
                continue;
            };

            return AreaFlags::from_bits_retain(old_raw);
        }
    }
}

impl Clone for AtomicFlags {
    fn clone(&self) -> Self {
        Self::new(self.load())
    }
}

impl RangeProtected {
    pub const fn new(range: VRange) -> Self {
        Self(UnsafeCell::new(range))
    }

    unsafe fn as_ref_unchecked<'a>(&self) -> &'a VRange {
        unsafe { &*self.0.get() }
    }

    pub fn as_ref<'a>(&self, _lock: impl RangeReadLock + 'a) -> &'a VRange {
        unsafe {
            // SAFETY: We are holding some kind of read safety lock.
            self.as_ref_unchecked()
        }
    }

    pub fn as_mut<'a>(
        &self, _mm_list_lock: &'a mut MemListLock,
        _list_write_lock: &'a mut AreaListLock, _area_lock: &'a mut AreaLock,
    ) -> &'a mut VRange {
        unsafe {
            // SAFETY: If we are holding the list's write lock, we can guarantee
            // that no one else is reading or writing the areas, so it's safe to
            // return a mutable reference.
            &mut *self.0.get()
        }
    }
}

impl AreaList {
    pub const fn new() -> Self {
        Self {
            areas: RBTree::new(AreasAdapter::NEW),
            lock: AreaListLock { _phantom: () },
        }
    }

    pub fn insert_new(&mut self, area: Arc<MemArea>) {
        let range = area.range.as_ref(&self.lock).clone();

        match self.areas.entry(&range) {
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

        let range = ub.range.as_ref(&self.lock);
        range.end() <= range.start()
    }

    /// Isolate the given range by splitting the areas that overlap with it, and
    /// return the areas that are fully covered by the range and removed.
    pub async fn isolate(
        &mut self, isolate_range: &VRange, lock: &mut MemListLock,
    ) -> impl Iterator<Item = Arc<MemArea>> {
        struct SendFuture<F>(F);
        unsafe impl<F> Send for SendFuture<F> {}
        unsafe impl<F> Sync for SendFuture<F> {}
        impl<F> core::future::Future for SendFuture<F>
        where
            F: core::future::Future,
        {
            type Output = F::Output;

            fn poll(
                self: core::pin::Pin<&mut Self>,
                cx: &mut core::task::Context<'_>,
            ) -> core::task::Poll<Self::Output> {
                unsafe { self.map_unchecked_mut(|s| &mut s.0).poll(cx) }
            }
        }

        SendFuture(self.isolate_(isolate_range, lock)).await
    }

    async fn isolate_(
        &mut self, isolate_range: &VRange, lock: &mut MemListLock,
    ) -> impl Iterator<Item = Arc<MemArea>> {
        let mut ret_areas = LinkedList::new(ListAdapter::NEW);
        let begin = VRange::from(isolate_range.start());

        let mut cursor = self.areas.upper_bound_mut(Bound::Included(&begin));

        while !cursor.is_null() {
            let range = {
                let area = cursor.get().unwrap();
                let range = area.range.as_ref(&*lock);

                if range.start() >= isolate_range.end() {
                    break;
                }

                if range.end() <= isolate_range.start() {
                    cursor.move_next();
                    continue;
                }

                range.clone()
            };

            let area = cursor.as_cursor().clone_pointer().unwrap();
            let list_lock = &mut self.lock;
            let mut area_lock = area.lock.lock().await;
            let area_lock = &mut area_lock;

            let (l, m, r) = range.mask_with_checked(isolate_range).unwrap();

            match (l, r) {
                (None, None) => {
                    // Fully covered, just remove it.
                    let area = cursor.remove().unwrap();
                    unsafe {
                        area.link.to_list();
                    }
                    ret_areas.push_back(area);
                    continue;
                }
                (None, Some(rem)) | (Some(rem), None) => {
                    // Overflow on one side, change the old area's range and
                    // return the newly created area.
                    let range = area.range.as_mut(lock, list_lock, area_lock);
                    *range = rem;
                }
                (Some(left), Some(right)) => {
                    // Overflow on both sides, change the old area's range to
                    // the left part, create a new area for the right part and
                    // return the middle part.
                    let range = area.range.as_mut(lock, list_lock, area_lock);
                    *range = left;

                    cursor.insert_after(area.clone_and_modify(lock, |area| {
                        area.range = RangeProtected::new(right);
                    }));
                }
            }

            let ret_range = area.clone_and_modify(lock, |area| {
                area.range = RangeProtected::new(m);
                area.link = Link::list();
            });

            ret_areas.push_back(ret_range);
        }

        // We are returning the area, and the area should be by default
        // in rbtrees, so we need to convert the link back to rbtree
        // link so the caller won't be surprised.
        ret_areas.into_iter().inspect(|area| unsafe {
            // SAFETY: `into_iter()` removes the area from the list.
            area.link.to_rbtree()
        })
    }

    // TODO: For backwards compatibility. Remove this.
    pub fn insert(&mut self, area: MemArea) {
        let range = area.range.as_ref(&self.lock).clone();

        match self.areas.entry(&range) {
            Entry::Occupied(_) => panic!("Overlapping mem area: {:?}.", range),
            Entry::Vacant(insert_cursor) => {
                insert_cursor.insert(Arc::new(area));
            }
        }
    }

    // TODO: For backwards compatibility. Remove this.
    pub fn deep_clone(&self, lock: &MemListLock) -> Self {
        let mut areas = RBTree::new(AreasAdapter::NEW);

        for area in self.areas.iter() {
            areas.insert(Arc::new(area.clone(lock)));
        }

        Self {
            areas,
            lock: AreaListLock { _phantom: () },
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &MemArea> {
        let mut cursor = self.areas.front();

        core::iter::from_fn(move || {
            let area = cursor.get()?;
            cursor.move_next();

            Some(area)
        })
    }
}

impl MemArea {
    pub const fn new(
        range: VRange, flags: AreaFlags, mapping: Mapping,
    ) -> Self {
        Self {
            range: RangeProtected::new(range),
            flags: AtomicFlags::new(flags),
            lock: Mutex::new(AreaLock::_new()),
            link: Link::rbtree(),
            mapping,
        }
    }

    pub fn set_permission(&self, perm: Permission) {
        let mut new_flags = AreaFlags::empty();

        if perm.read {
            new_flags.insert(AreaFlags::READ);
        }

        if perm.write {
            new_flags.insert(AreaFlags::WRITE);
        }

        if perm.execute {
            new_flags.insert(AreaFlags::EXECUTE);
        }

        self.flags
            .update(|flags| (flags & !AreaFlags::RWX) | new_flags);
    }

    pub fn is_shared(&self) -> bool {
        self.flags.load().contains(AreaFlags::SHARED)
    }

    pub fn can_read(&self) -> bool {
        self.flags.load().contains(AreaFlags::READ)
    }

    pub fn can_write(&self) -> bool {
        self.flags.load().contains(AreaFlags::WRITE)
    }

    pub fn can_execute(&self) -> bool {
        self.flags.load().contains(AreaFlags::EXECUTE)
    }

    pub fn clone(&self, lock: &MemListLock) -> Self {
        Self {
            range: RangeProtected::new(self.range.as_ref(lock).clone()),
            flags: self.flags.clone(),
            lock: Mutex::new(AreaLock::_new()),
            link: Link::rbtree(),
            mapping: self.mapping.clone(),
        }
    }

    fn clone_and_modify(
        &self, lock: &MemListLock, modify: impl FnOnce(&mut MemArea),
    ) -> Arc<Self> {
        let mut arc = Arc::new(self.clone(lock));
        let arc_mut = unsafe {
            // SAFETY: We are the only owner.
            Arc::get_mut(&mut arc).unwrap_unchecked()
        };
        modify(arc_mut);
        arc
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
        // Exclude concurrent modifications and faults.
        // TODO: concurrent faults should be acceptable...
        let lock = self.lock.lock().await;
        assert!(
            offset < self.range.as_ref(&*lock).len(),
            "Offset out of range"
        );

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

impl Link {
    const fn rbtree() -> Self {
        Self {
            rbtree: ManuallyDrop::new(RBTreeAtomicLink::new()),
        }
    }

    const fn list() -> Self {
        Self {
            list: ManuallyDrop::new(LinkedListAtomicLink::new()),
        }
    }

    /// Convert from RBTree link to LinkedList link.
    ///
    /// # Safety
    /// The caller must guarantee that the link is currently not linked.
    /// Otherwise, this is an undefined behavior.
    unsafe fn to_list(&self) {
        unsafe {
            self.list.force_unlink();
        }
    }

    /// Convert from LinkedList link to RBTree link.
    ///
    /// # Safety
    /// The caller must guarantee that the link is currently not linked.
    /// Otherwise, this is an undefined behavior.
    unsafe fn to_rbtree(&self) {
        unsafe {
            self.rbtree.force_unlink();
        }
    }
}
