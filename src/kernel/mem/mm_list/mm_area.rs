use alloc::sync::Arc;
use core::cell::UnsafeCell;
use core::mem::ManuallyDrop;
use core::sync::atomic::{AtomicU32, Ordering};

use eonix_mm::address::{VAddr, VRange};
use eonix_mm::page_table::{PageAttribute, RawAttribute, PTE};
use eonix_mm::paging::PFN;
use eonix_sync::Mutex;
use intrusive_collections::rbtree::Entry;
use intrusive_collections::{
    intrusive_adapter, Bound, KeyAdapter, LinkedList, LinkedListAtomicLink,
    PointerOps, RBTree, RBTreeAtomicLink,
};

use super::Mapping;
use crate::kernel::mem::mm_list::mapping::{add_mapping, AnonMapping};
use crate::kernel::mem::mm_list::page_table::KernelPageTable;
use crate::kernel::mem::mm_list::{
    remove_mapping, FileMapping, MemListLock, PageTableExt,
};
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

    pub fn insert(&mut self, area: Arc<MemArea>) {
        let lock = area.lock.try_lock().expect("Insert called on busy area");

        match self.areas.entry(area.range.as_ref(&*lock)) {
            Entry::Occupied(_) => {
                panic!("Overlapping mem area: {:?}.", area.range.as_ref(&*lock))
            }
            Entry::Vacant(insert_cursor) => {
                drop(lock);
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
        let mut cursor = self.areas.upper_bound_mut(Bound::Included(
            &VRange::from(isolate_range.start()),
        ));

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

            let mut area = cursor.remove().unwrap();

            {
                // Exclude all concurrent faulters.
                area.lock.lock().await;

                // XXX: This is slightly racy and the assertion may not hold if
                //      one hasn't dropped the arc after releasing the lock.
                //      Let's observe if we can actually trigger this...
                assert_eq!(Arc::strong_count(&area), 1, "Shared isolated area");
            }

            let left_overflow = range.start() < isolate_range.start();
            let right_overflow = range.end() > isolate_range.end();

            if left_overflow {
                let left;
                let offset = isolate_range.start() - range.start();

                (left, area) = area.split(offset, lock);

                cursor.insert_before(left);
            }

            if right_overflow {
                let right;
                let offset = isolate_range.end() - range.start();

                (area, right) = area.split(offset, lock);

                cursor.insert_before(right);
            }

            unsafe {
                // SAFETY: The area is unlinked and exclusive to us.
                area.link.to_list();
            }
            ret_areas.push_back(area);
        }

        // We are returning the area, and the area should be by default
        // in rbtrees, so we need to convert the link back to rbtree
        // link so the caller won't be surprised.
        ret_areas.into_iter().inspect(|area| unsafe {
            // SAFETY: `into_iter()` removes the area from the list.
            area.link.to_rbtree()
        })
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
    pub fn new(range: VRange, flags: AreaFlags, mapping: Mapping) -> Arc<Self> {
        Arc::new(Self {
            range: RangeProtected::new(range),
            flags: AtomicFlags::new(flags),
            lock: Mutex::new(AreaLock::_new()),
            link: Link::rbtree(),
            mapping,
        })
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

    pub fn clone(&self, lock: &MemListLock) -> Arc<Self> {
        Self::new(
            self.range.as_ref(lock).clone(),
            self.flags.load(),
            self.mapping.clone(),
        )
    }

    fn split(
        &self, offset: usize, lock: &MemListLock,
    ) -> (Arc<Self>, Arc<Self>) {
        let (begin, mid, end) = {
            let range = self.range.as_ref(lock);
            let begin = range.start();
            let mid = begin + offset;
            let end = range.end();

            (begin, mid, end)
        };

        let flags = self.flags.load();
        let (left_mapping, right_mapping) = self.mapping.split(offset);

        let left = Self::new(VRange::new(begin, mid), flags, left_mapping);
        let right = Self::new(VRange::new(mid, end), flags, right_mapping);

        (left, right)
    }

    fn handle_cow(&self, pfn: &mut PFN, attr: &mut PageAttribute) {
        assert!(attr.contains(PageAttribute::COPY_ON_WRITE));

        attr.remove(PageAttribute::COPY_ON_WRITE);
        attr.set(PageAttribute::WRITE, self.can_write());

        let folio = unsafe {
            // SAFETY: `pfn` is a mapping taken from some page table.
            remove_mapping(*pfn)
        };

        // XXX: Change me!!!
        if folio.refcount.load(Ordering::Relaxed) == 1 {
            // SAFETY: This is actually safe. If we read `1` here and we have `MMList` lock
            // held, there couldn't be neither other processes sharing the page, nor other
            // threads making the page COW at the same time.
            *pfn = add_mapping(folio);
            return;
        }

        let mut new_page = FolioOwned::alloc();

        unsafe {
            // SAFETY: `folio` is CoW, which means that others won't write to it.
            let old_page_data = folio.get_bytes_ptr().as_ref();
            let new_page_data = new_page.as_bytes_mut();

            new_page_data.copy_from_slice(old_page_data);
        };

        attr.remove(PageAttribute::ACCESSED);
        *pfn = add_mapping(new_page.share());
    }

    async fn missing_file(
        &self, pfn: &mut PFN, attr: &mut PageAttribute, offset: PageOffset,
        write: bool, file_mapping: &FileMapping,
        anon_mapping: Option<&AnonMapping>,
    ) -> KResult<()> {
        assert!(
            offset.byte_count() < file_mapping.length,
            "Offset out of range"
        );
        assert!(!attr.contains(PageAttribute::PRESENT));

        let file_offset = file_mapping.offset + offset;

        let map_page = |cache_page: &CachePage| {
            if !self.can_write() {
                assert!(!write, "Write fault on read-only mapping");

                *pfn = cache_page.add_mapping();
                return;
            }

            let Some(anon_mapping) = anon_mapping else {
                // We don't process dirty flags in write faults.
                // Simply assume that page will eventually be dirtied.
                // So here we can set the dirty flag now.
                cache_page.set_dirty(true);
                attr.insert(PageAttribute::WRITE);
                *pfn = cache_page.add_mapping();
                return;
            };

            if !write {
                // Delay the copy-on-write until write fault happens.
                attr.insert(PageAttribute::COPY_ON_WRITE);
                *pfn = cache_page.add_mapping();
                return;
            }

            // Nah, we are writing to a mapped private mapping...
            let mut new_page = anon_mapping.alloc_folio();
            new_page
                .as_bytes_mut()
                .copy_from_slice(cache_page.lock().as_bytes());

            attr.insert(PageAttribute::WRITE);
            *pfn = add_mapping(new_page.share());
        };

        file_mapping
            .page_cache
            .with_page(file_offset, map_page)
            .await?;

        attr.insert(PageAttribute::PRESENT);

        if self.can_read() {
            attr.insert(PageAttribute::READ);
        }

        if self.can_execute() {
            attr.insert(PageAttribute::EXECUTE);
        }

        Ok(())
    }

    fn missing_anon(
        &self, pfn: &mut PFN, attr: &mut PageAttribute,
        anon_mapping: &AnonMapping,
    ) {
        let mut folio = anon_mapping.alloc_folio();
        folio.as_bytes_mut().fill(0);

        *pfn = add_mapping(folio.share());

        attr.insert(PageAttribute::PRESENT);

        if self.can_read() {
            attr.insert(PageAttribute::READ);
        }

        if self.can_execute() {
            attr.insert(PageAttribute::EXECUTE);
        }

        if self.can_write() {
            attr.insert(PageAttribute::WRITE);
        }
    }

    async fn handle_missing(
        &self, pfn: &mut PFN, attr: &mut PageAttribute, offset: PageOffset,
        write: bool,
    ) -> KResult<()> {
        assert!(
            !attr.contains(PageAttribute::COPY_ON_WRITE),
            "Missing PTEs should not have CoW set"
        );

        attr.insert(PageAttribute::USER);

        match &self.mapping {
            Mapping::Anonymous(anon_mapping) => {
                self.missing_anon(pfn, attr, anon_mapping);
            }
            Mapping::PrivateFile { file, anon } => {
                self.missing_file(pfn, attr, offset, write, file, Some(anon))
                    .await?;
            }
            Mapping::SharedFile(file_mapping) => {
                self.missing_file(pfn, attr, offset, write, file_mapping, None)
                    .await?;
            }
        }

        assert!(attr.contains(PageAttribute::PRESENT));

        Ok(())
    }

    fn handle_non_missing(
        &self, pfn: &mut PFN, attr: &mut PageAttribute,
    ) -> KResult<()> {
        if attr.contains(PageAttribute::COPY_ON_WRITE) {
            self.handle_cow(pfn, attr);
        }

        Ok(())
    }

    pub async fn handle(
        &self, pte: &mut impl PTE, offset: usize, write: bool,
    ) -> KResult<()> {
        let offset = PageOffset::from_byte_aligned(offset);

        // Exclude concurrent modifications and faults.
        // TODO: concurrent faults should be acceptable...
        let lock = self.lock.lock().await;
        assert!(
            offset.byte_count() < self.range.as_ref(&*lock).len(),
            "Offset out of range"
        );

        let (mut pfn, raw_attr) = pte.get();
        let mut attr = raw_attr.as_page_attr().expect("Not a page");

        if !attr.contains(PageAttribute::PRESENT) {
            self.handle_missing(&mut pfn, &mut attr, offset, write)
                .await?;
        } else {
            self.handle_non_missing(&mut pfn, &mut attr)?;
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

fn dup_area_shared<'a, 'b: 'a>(
    area: &'a MemArea, list: &mut AreaList, from_lock: &'b MemListLock,
) {
    // Shared areas can be filled in faults anyway.
    // Just cloning the area is enough.
    list.insert(area.clone(from_lock));
}

fn dup_area_private<'a, 'b: 'a>(
    area: &'a MemArea, list: &mut AreaList, from_lock: &'b MemListLock,
    from_pgtable: &KernelPageTable, to_pgtable: &KernelPageTable,
) {
    list.insert(area.clone(from_lock));

    to_pgtable.set_copy_on_write(from_pgtable, *area.range.as_ref(from_lock));
}

pub fn dup_area_to_list<'a, 'b: 'a>(
    area: &'a MemArea, list: &mut AreaList, from_lock: &'b MemListLock,
    from_pgtable: &KernelPageTable, to_pgtable: &KernelPageTable,
) {
    if area.is_shared() {
        dup_area_shared(area, list, from_lock);
        return;
    }

    dup_area_private(area, list, from_lock, from_pgtable, to_pgtable);
}
