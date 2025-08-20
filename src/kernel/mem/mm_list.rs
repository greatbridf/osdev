mod mapping;
mod page_fault;

use super::address::{VAddrExt as _, VRangeExt as _};
use super::page_alloc::GlobalPageAlloc;
use super::paging::AllocZeroed as _;
use super::{AsMemoryBlock, MMArea, Page};
use crate::kernel::constants::{EEXIST, EFAULT, EINVAL, ENOMEM};
use crate::kernel::mem::page_alloc::RawPagePtr;
use crate::{prelude::*, sync::ArcSwap};
use alloc::collections::btree_set::BTreeSet;
use core::fmt;
use core::sync::atomic::{AtomicUsize, Ordering};
use eonix_hal::mm::{
    flush_tlb_all, get_root_page_table_pfn, set_root_page_table_pfn, ArchPagingMode,
    ArchPhysAccess, GLOBAL_PAGE_TABLE,
};
use eonix_mm::address::PAddr;
use eonix_mm::page_table::PageAttribute;
use eonix_mm::paging::PFN;
use eonix_mm::{
    address::{AddrOps as _, VAddr, VRange},
    page_table::{PageTable, RawAttribute, PTE},
    paging::PAGE_SIZE,
};
use eonix_sync::{LazyLock, Mutex};

pub use mapping::{FileMapping, Mapping};
pub use page_fault::handle_kernel_page_fault;

pub static EMPTY_PAGE: LazyLock<Page> = LazyLock::new(|| Page::zeroed());

#[derive(Debug, Clone, Copy)]
pub struct Permission {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

type KernelPageTable<'a> = PageTable<'a, ArchPagingMode, GlobalPageAlloc, ArchPhysAccess>;

struct MMListLocked {
    areas: BTreeSet<MMArea>,
    break_start: Option<VRange>,
    break_pos: Option<VAddr>,
}

struct MMListInner {
    user_count: AtomicUsize,
    page_table: KernelPageTable<'static>,

    locked: Mutex<MMListLocked>,
}

pub struct MMList {
    inner: ArcSwap<MMListInner>,
}

trait AreasList {
    fn overlapping_addr(&self, addr: VAddr) -> Option<&MMArea>;
    fn check_overlapping_addr(&self, addr: VAddr) -> bool;
    fn overlapping_range(&self, range: VRange) -> impl DoubleEndedIterator<Item = &MMArea> + '_;
    fn check_overlapping_range(&self, range: VRange) -> bool;
}

impl AreasList for BTreeSet<MMArea> {
    fn overlapping_addr(&self, addr: VAddr) -> Option<&MMArea> {
        self.get(&VRange::from(addr))
    }

    fn check_overlapping_addr(&self, addr: VAddr) -> bool {
        addr.is_user() && self.overlapping_addr(addr).is_none()
    }

    fn overlapping_range(&self, range: VRange) -> impl DoubleEndedIterator<Item = &MMArea> + '_ {
        self.range(range.into_bounds())
    }

    fn check_overlapping_range(&self, range: VRange) -> bool {
        range.is_user() && self.overlapping_range(range).next().is_none()
    }
}

impl MMListLocked {
    fn random_start(&self) -> VAddr {
        VAddr::from(0x1234000)
    }

    fn find_available(&self, mut hint: VAddr, len: usize) -> Option<VAddr> {
        let len = len.div_ceil(PAGE_SIZE) * PAGE_SIZE;

        if hint == VAddr::NULL {
            hint = self.random_start();
        } else {
            hint = hint.floor();
        }

        let mut range = VRange::from(hint).grow(len);

        loop {
            if !range.is_user() {
                return None;
            }

            match self.areas.overlapping_range(range).next_back() {
                None => return Some(range.start()),
                Some(area) => {
                    range = VRange::from(area.range().end().ceil()).grow(len);
                }
            }
        }
    }
}

impl MMListInner {
    async fn unmap(&self, start: VAddr, len: usize) -> KResult<Vec<Page>> {
        assert_eq!(start.floor(), start);
        let end = (start + len).ceil();
        let range_to_unmap = VRange::new(start, end);
        if !range_to_unmap.is_user() {
            return Err(EINVAL);
        }

        let mut left_remaining = None;
        let mut right_remaining = None;

        let mut pages_to_free = Vec::new();

        // TODO: Write back dirty pages.

        let mut locked = self.locked.lock().await;

        locked.areas.retain(|area| {
            let Some((left, mid, right)) = area.range().mask_with_checked(&range_to_unmap) else {
                return true;
            };

            for pte in self.page_table.iter_user(mid) {
                let (pfn, _) = pte.take();
                pages_to_free.push(unsafe {
                    // SAFETY: We got the pfn from a valid page table entry, so it should be valid.
                    Page::from_raw(pfn)
                });
            }

            match (left, right) {
                (None, None) => {}
                (Some(left), None) => {
                    assert!(left_remaining.is_none());
                    let (Some(left), _) = area.clone().split(left.end()) else {
                        unreachable!("`left.end()` is within the area");
                    };

                    left_remaining = Some(left);
                }
                (None, Some(right)) => {
                    assert!(right_remaining.is_none());
                    let (_, Some(right)) = area.clone().split(right.start()) else {
                        unreachable!("`right.start()` is within the area");
                    };

                    right_remaining = Some(right);
                }
                (Some(left), Some(right)) => {
                    assert!(left_remaining.is_none());
                    assert!(right_remaining.is_none());
                    let (Some(left), Some(mid)) = area.clone().split(left.end()) else {
                        unreachable!("`left.end()` is within the area");
                    };

                    let (_, Some(right)) = mid.split(right.start()) else {
                        unreachable!("`right.start()` is within the area");
                    };

                    left_remaining = Some(left);
                    right_remaining = Some(right);
                }
            }

            false
        });

        if let Some(front) = left_remaining {
            locked.areas.insert(front);
        }
        if let Some(back) = right_remaining {
            locked.areas.insert(back);
        }

        Ok(pages_to_free)
    }

    async fn protect(&self, start: VAddr, len: usize, permission: Permission) -> KResult<()> {
        assert_eq!(start.floor(), start);
        assert!(len != 0);

        let end = (start + len).ceil();
        let range_to_protect = VRange::new(start, end);
        if !range_to_protect.is_user() {
            return Err(EINVAL);
        }

        let mut locked = self.locked.lock().await;

        let mut found = false;
        let old_areas = core::mem::take(&mut locked.areas);
        for mut area in old_areas {
            let Some((left, mid, right)) = area.range().mask_with_checked(&range_to_protect) else {
                locked.areas.insert(area);
                continue;
            };

            found = true;

            if let Some(left) = left {
                let (Some(left), Some(right)) = area.split(left.end()) else {
                    unreachable!("`left.end()` is within the area");
                };

                locked.areas.insert(left);
                area = right;
            }

            if let Some(right) = right {
                let (Some(left), Some(right)) = area.split(right.start()) else {
                    unreachable!("`right.start()` is within the area");
                };

                locked.areas.insert(right);
                area = left;
            }

            for pte in self.page_table.iter_user(mid) {
                let mut page_attr = pte.get_attr().as_page_attr().expect("Not a page attribute");

                if !permission.read && !permission.write && !permission.execute {
                    // If no permissions are set, we just remove the page.
                    page_attr.remove(
                        PageAttribute::PRESENT
                            | PageAttribute::READ
                            | PageAttribute::WRITE
                            | PageAttribute::EXECUTE,
                    );

                    pte.set_attr(page_attr.into());
                    continue;
                }

                page_attr.set(PageAttribute::READ, permission.read);

                if !page_attr.contains(PageAttribute::COPY_ON_WRITE) {
                    page_attr.set(PageAttribute::WRITE, permission.write);
                }

                page_attr.set(PageAttribute::EXECUTE, permission.execute);

                pte.set_attr(page_attr.into());
            }

            area.permission = permission;
            locked.areas.insert(area);
        }

        if !found {
            return Err(ENOMEM);
        }

        Ok(())
    }

    async fn mmap(
        &self,
        at: VAddr,
        len: usize,
        mapping: Mapping,
        permission: Permission,
        is_shared: bool,
    ) -> KResult<()> {
        assert_eq!(at.floor(), at);
        assert_eq!(len & (PAGE_SIZE - 1), 0);
        let range = VRange::new(at, at + len);

        let mut locked = self.locked.lock().await;

        // We are doing a area marker insertion.
        if len == 0 && !locked.areas.check_overlapping_addr(at)
            || !locked.areas.check_overlapping_range(range)
        {
            return Err(EEXIST);
        }

        match &mapping {
            Mapping::Anonymous => self.page_table.set_anonymous(range, permission),
            Mapping::File(_) => self.page_table.set_mmapped(range, permission),
        }

        locked
            .areas
            .insert(MMArea::new(range, mapping, permission, is_shared));
        Ok(())
    }
}

impl Drop for MMListInner {
    fn drop(&mut self) {
        // May buggy
        let locked = self.locked.get_mut();

        for area in &locked.areas {
            if area.is_shared {
                for pte in self.page_table.iter_user(area.range()) {
                    let (pfn, _) = pte.take();
                    let raw_page = RawPagePtr::from(pfn);
                    if raw_page.refcount().fetch_sub(1, Ordering::Relaxed) == 1 {
                        // Wrong here
                        // unsafe { Page::from_raw(pfn) };
                    }
                }
            } else {
                for pte in self.page_table.iter_user(area.range()) {
                    let (pfn, _) = pte.take();
                    unsafe { Page::from_raw(pfn) };
                }
            }
        }

        // TODO: Recycle all pages in the page table.
    }
}

impl MMList {
    async fn flush_user_tlbs(&self) {
        match self.inner.borrow().user_count.load(Ordering::Relaxed) {
            0 => {
                // If there are currently no users, we don't need to do anything.
            }
            1 => {
                if PAddr::from(get_root_page_table_pfn()) == self.inner.borrow().page_table.addr() {
                    // If there is only one user and we are using the page table,
                    // flushing the TLB for the local cpu only is enough.
                    flush_tlb_all();
                } else {
                    // Send the TLB flush request to the core.
                    todo!();
                }
            }
            _ => {
                // If there are more than one users, we broadcast the TLB flush
                // to all cores.
                todo!()
            }
        }
    }

    pub fn new() -> Self {
        let page_table = GLOBAL_PAGE_TABLE.clone_global();
        Self {
            inner: ArcSwap::new(MMListInner {
                user_count: AtomicUsize::new(0),
                page_table,
                locked: Mutex::new(MMListLocked {
                    areas: BTreeSet::new(),
                    break_start: None,
                    break_pos: None,
                }),
            }),
        }
    }

    pub async fn new_cloned(&self) -> Self {
        let inner = self.inner.borrow();
        let locked = inner.locked.lock().await;

        let page_table = GLOBAL_PAGE_TABLE.clone_global();
        let list = Self {
            inner: ArcSwap::new(MMListInner {
                user_count: AtomicUsize::new(0),
                page_table,
                locked: Mutex::new(MMListLocked {
                    areas: locked.areas.clone(),
                    break_start: locked.break_start,
                    break_pos: locked.break_pos,
                }),
            }),
        };

        let list_inner = list.inner.borrow();

        for area in locked.areas.iter() {
            if !area.is_shared {
                list_inner
                    .page_table
                    .set_copy_on_write(&inner.page_table, area.range());
            } else {
                // may buggy here
                list_inner
                    .page_table
                    .set_copied(&inner.page_table, area.range());
            }
        }

        // We've set some pages as CoW, so we need to invalidate all our users' TLB.
        self.flush_user_tlbs().await;

        list
    }

    pub async fn new_shared(&self) -> Self {
        let inner = self.inner.borrow();

        Self {
            inner: ArcSwap::with_pointer(inner.clone()),
        }
    }

    pub fn activate(&self) {
        if let Some(inner) = self.inner.try_borrow() {
            inner.user_count.fetch_add(1, Ordering::Acquire);
            set_root_page_table_pfn(PFN::from(inner.page_table.addr()));
        }
    }

    pub fn deactivate(&self) {
        if let Some(inner) = self.inner.try_borrow() {
            let old_user_count = inner.user_count.fetch_sub(1, Ordering::Release);
            assert_ne!(old_user_count, 0);
        }
    }

    /// Replace the current page table with a new one.
    ///
    /// # Safety
    /// This function should be called only when we are sure that the `MMList` is not
    /// being used by any other thread.
    pub unsafe fn replace(&self, new: Option<Self>) {
        eonix_preempt::disable();

        let me_inner = self.inner.borrow();

        let current_root_page_table = PAddr::from(get_root_page_table_pfn());

        assert_eq!(
            new.as_ref()
                .map(|new_mm| new_mm.inner.borrow().user_count.load(Ordering::Relaxed))
                .unwrap_or(0),
            0,
            "`new` must not be used by anyone"
        );

        if me_inner.page_table.addr() == current_root_page_table {
            self.deactivate();

            if let Some(new) = &new {
                new.activate();
            } else {
                set_root_page_table_pfn(PFN::from(GLOBAL_PAGE_TABLE.addr()));
            }
        }

        // TODO: Check whether we should wake someone up if they've been put
        //       to sleep when calling `vfork`.
        self.inner
            .swap(new.map(|new_mm| new_mm.inner.swap(None)).flatten());

        eonix_preempt::enable();
    }

    /// No need to do invalidation manually, `PageTable` already does it.
    pub async fn unmap(&self, start: VAddr, len: usize) -> KResult<()> {
        let pages_to_free = self.inner.borrow().unmap(start, len).await?;

        // We need to assure that the pages are not accessed anymore.
        // The ones having these pages in their TLB could read from or write to them.
        // So flush the TLBs first for all our users.
        self.flush_user_tlbs().await;

        // Then free the pages.
        drop(pages_to_free);

        Ok(())
    }

    pub async fn protect(&self, start: VAddr, len: usize, prot: Permission) -> KResult<()> {
        self.inner.borrow().protect(start, len, prot).await?;

        // flush the tlb due to the pte attribute changes
        self.flush_user_tlbs().await;

        Ok(())
    }

    pub async fn map_vdso(&self) -> KResult<()> {
        unsafe extern "C" {
            fn VDSO_PADDR();
        }
        static VDSO_PADDR_VALUE: &'static unsafe extern "C" fn() =
            &(VDSO_PADDR as unsafe extern "C" fn());

        let vdso_paddr = unsafe {
            // SAFETY: To prevent the compiler from optimizing this into `la` instructions
            //         and causing a linking error.
            (VDSO_PADDR_VALUE as *const _ as *const usize).read_volatile()
        };

        let vdso_pfn = PFN::from(PAddr::from(vdso_paddr));

        const VDSO_START: VAddr = VAddr::from(0x7f00_0000_0000);
        const VDSO_SIZE: usize = 0x1000;

        let inner = self.inner.borrow();

        let mut pte_iter = inner
            .page_table
            .iter_user(VRange::from(VDSO_START).grow(VDSO_SIZE));

        let pte = pte_iter.next().expect("There should be at least one PTE");
        pte.set(
            vdso_pfn,
            (PageAttribute::PRESENT
                | PageAttribute::READ
                | PageAttribute::EXECUTE
                | PageAttribute::USER
                | PageAttribute::ACCESSED)
                .into(),
        );

        assert!(pte_iter.next().is_none(), "There should be only one PTE");

        Ok(())
    }

    pub async fn mmap_hint(
        &self,
        hint: VAddr,
        len: usize,
        mapping: Mapping,
        permission: Permission,
        is_shared: bool,
    ) -> KResult<VAddr> {
        let inner = self.inner.borrow();

        if hint == VAddr::NULL {
            let at = inner
                .locked
                .lock()
                .await
                .find_available(hint, len)
                .ok_or(ENOMEM)?;
            inner.mmap(at, len, mapping, permission, is_shared).await?;
            return Ok(at);
        }

        match inner
            .mmap(hint, len, mapping.clone(), permission, is_shared)
            .await
        {
            Ok(()) => Ok(hint),
            Err(EEXIST) => {
                let at = inner
                    .locked
                    .lock()
                    .await
                    .find_available(hint, len)
                    .ok_or(ENOMEM)?;

                inner.mmap(at, len, mapping, permission, is_shared).await?;
                Ok(at)
            }
            Err(err) => Err(err),
        }
    }

    pub async fn mmap_fixed(
        &self,
        at: VAddr,
        len: usize,
        mapping: Mapping,
        permission: Permission,
        is_shared: bool,
    ) -> KResult<VAddr> {
        self.inner
            .borrow()
            .mmap(at, len, mapping.clone(), permission, is_shared)
            .await
            .map(|_| at)
    }

    pub async fn set_break(&self, pos: Option<VAddr>) -> VAddr {
        let inner = self.inner.borrow();
        let mut locked = inner.locked.lock().await;

        // SAFETY: `set_break` is only called in syscalls, where program break should be valid.
        assert!(locked.break_start.is_some() && locked.break_pos.is_some());
        let current_break = locked.break_pos.unwrap();
        let pos = match pos {
            None => return current_break,
            Some(pos) => pos.ceil(),
        };

        if current_break > pos {
            return current_break;
        }

        let range = VRange::new(current_break, pos);
        if !locked.areas.check_overlapping_range(range) {
            return current_break;
        }

        locked.break_pos = Some(pos);

        drop(locked);

        let len = pos - current_break;

        let _ = self
            .mmap_fixed(
                current_break,
                len,
                Mapping::Anonymous,
                Permission {
                    read: true,
                    write: true,
                    execute: false,
                },
                false,
            )
            .await
            .unwrap();

        pos
    }

    /// This should be called only **once** for every thread.
    pub async fn register_break(&self, start: VAddr) {
        let inner = self.inner.borrow();
        let mut locked = inner.locked.lock().await;
        assert!(locked.break_start.is_none() && locked.break_pos.is_none());

        locked.break_start = Some(start.into());
        locked.break_pos = Some(start);
    }

    /// Access the memory area with the given function.
    /// The function will be called with the offset of the area and the slice of the area.
    pub async fn access_mut<F>(&self, start: VAddr, len: usize, func: F) -> KResult<()>
    where
        F: Fn(usize, &mut [u8]),
    {
        // First, validate the address range.
        let end = start + len;
        if !start.is_user() || !end.is_user() {
            return Err(EINVAL);
        }

        let inner = self.inner.borrow();
        let locked = inner.locked.lock().await;

        let mut offset = 0;
        let mut remaining = len;
        let mut current = start;

        while remaining > 0 {
            let area = locked.areas.overlapping_addr(current).ok_or(EFAULT)?;

            let area_start = area.range().start();
            let area_end = area.range().end();
            let area_remaining = area_end - current;

            let access_len = remaining.min(area_remaining);
            let access_end = current + access_len;

            for (idx, pte) in inner
                .page_table
                .iter_user(VRange::new(current, access_end))
                .enumerate()
            {
                let page_start = current.floor() + idx * 0x1000;
                let page_end = page_start + 0x1000;

                // Prepare for the worst case that we might write to the page...
                area.handle(pte, page_start - area_start, true).await?;

                let start_offset;
                if page_start < current {
                    start_offset = current - page_start;
                } else {
                    start_offset = 0;
                }

                let end_offset;
                if page_end > access_end {
                    end_offset = access_end - page_start;
                } else {
                    end_offset = 0x1000;
                }

                unsafe {
                    // SAFETY: We are sure that the page is valid and we have the right to access it.
                    Page::with_raw(pte.get_pfn(), |page| {
                        // SAFETY: The caller guarantees that no one else is using the page.
                        let page_data = page.as_memblk().as_bytes_mut();
                        func(
                            offset + idx * 0x1000,
                            &mut page_data[start_offset..end_offset],
                        );
                    });
                }
            }

            offset += access_len;
            remaining -= access_len;
            current = access_end;
        }

        Ok(())
    }
}

impl fmt::Debug for MMList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MMList").finish()
    }
}

trait PageTableExt {
    fn set_anonymous(&self, range: VRange, permission: Permission);
    fn set_mmapped(&self, range: VRange, permission: Permission);
    fn set_copy_on_write(&self, from: &Self, range: VRange);
    fn set_copied(&self, from: &Self, range: VRange);
}

impl PageTableExt for KernelPageTable<'_> {
    fn set_anonymous(&self, range: VRange, permission: Permission) {
        for pte in self.iter_user(range) {
            pte.set_anonymous(permission.execute);
        }
    }

    fn set_mmapped(&self, range: VRange, permission: Permission) {
        for pte in self.iter_user(range) {
            pte.set_mapped(permission.execute);
        }
    }

    fn set_copy_on_write(&self, from: &Self, range: VRange) {
        let to_iter = self.iter_user(range);
        let from_iter = from.iter_user(range);

        for (to, from) in to_iter.zip(from_iter) {
            to.set_copy_on_write(from);
        }
    }

    fn set_copied(&self, from: &Self, range: VRange) {
        let to_iter = self.iter_user(range);
        let from_iter = from.iter_user(range);

        for (to, from) in to_iter.zip(from_iter) {
            let (pfn, attr) = from.get();
            to.set(
                unsafe { Page::with_raw(pfn, |page| page.clone()) }.into_raw(),
                attr,
            );
        }
    }
}

trait PTEExt {
    // private anonymous
    fn set_anonymous(&mut self, execute: bool);
    // file mapped or shared anonymous
    fn set_mapped(&mut self, execute: bool);
    fn set_copy_on_write(&mut self, from: &mut Self);
}

impl<T> PTEExt for T
where
    T: PTE,
{
    fn set_anonymous(&mut self, execute: bool) {
        // Writable flag is set during page fault handling while executable flag is
        // preserved across page faults, so we set executable flag now.
        let mut attr = PageAttribute::PRESENT
            | PageAttribute::READ
            | PageAttribute::USER
            | PageAttribute::COPY_ON_WRITE;
        attr.set(PageAttribute::EXECUTE, execute);

        self.set(EMPTY_PAGE.clone().into_raw(), T::Attr::from(attr));
    }

    fn set_mapped(&mut self, execute: bool) {
        // Writable flag is set during page fault handling while executable flag is
        // preserved across page faults, so we set executable flag now.
        let mut attr = PageAttribute::READ | PageAttribute::USER | PageAttribute::MAPPED;
        attr.set(PageAttribute::EXECUTE, execute);

        self.set(EMPTY_PAGE.clone().into_raw(), T::Attr::from(attr));
    }

    fn set_copy_on_write(&mut self, from: &mut Self) {
        let mut from_attr = from
            .get_attr()
            .as_page_attr()
            .expect("Not a page attribute");

        if !from_attr.intersects(PageAttribute::PRESENT | PageAttribute::MAPPED) {
            return;
        }

        from_attr.remove(PageAttribute::WRITE | PageAttribute::DIRTY);
        from_attr.insert(PageAttribute::COPY_ON_WRITE);

        let pfn = unsafe {
            // SAFETY: We get the pfn from a valid page table entry, so it should be valid as well.
            Page::with_raw(from.get_pfn(), |page| page.clone().into_raw())
        };

        self.set(pfn, T::Attr::from(from_attr & !PageAttribute::ACCESSED));

        from.set_attr(T::Attr::from(from_attr));
    }
}
