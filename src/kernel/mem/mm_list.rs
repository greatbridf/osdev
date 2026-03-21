mod brk;
mod mapping;
mod mm_area;
mod page_fault;
mod page_table;

use alloc::sync::Arc;
use core::fmt;
use core::sync::atomic::{AtomicUsize, Ordering};

use eonix_hal::mm::{
    flush_tlb_all, get_root_page_table_pfn, set_root_page_table_pfn,
    GLOBAL_PAGE_TABLE,
};
use eonix_mm::address::{Addr as _, AddrOps as _, PAddr, VAddr, VRange};
use eonix_mm::page_table::{PageAttribute, RawAttribute, PTE};
use eonix_mm::paging::{Folio as _, PAGE_SIZE, PFN};
use eonix_sync::Mutex;
use mm_area::AreaList;
use page_table::KernelPageTable;

pub use self::mapping::{
    add_mapping, duplicate_mapping, remove_mapping, AnonMapping, FileMapping,
    Mapping,
};
pub use self::mm_area::{AreaFlags, MemArea};
pub use self::page_fault::handle_kernel_page_fault;
use super::address::{VAddrExt as _, VRangeExt as _};
use super::Folio;
use crate::kernel::constants::{EEXIST, EFAULT, EINVAL, ENOMEM};
use crate::kernel::mem::mm_list::brk::ProgramBreak;
use crate::kernel::mem::mm_list::mm_area::dup_area_to_list;
use crate::prelude::*;
use crate::sync::ArcSwap;

#[derive(Debug, Clone, Copy)]
pub struct Permission {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

pub struct MemListLock {
    _phantom: (),
}

pub struct MMListInner {
    pub lock: MemListLock,
    areas: AreaList,
    page_table: KernelPageTable,
    prog_break: ProgramBreak,
}

pub struct MMList {
    inner: ArcSwap<Mutex<MMListInner>>,
    user_count: AtomicUsize,
    /// Only used in kernel space to switch page tables on context switch.
    root_page_table: AtomicUsize,
}

impl MemListLock {
    const fn _new() -> Self {
        Self { _phantom: () }
    }
}

impl MMListInner {
    fn overlapping_addr(&self, addr: VAddr) -> Option<Arc<MemArea>> {
        self.areas.get(addr)
    }

    fn check_overlapping_addr(&self, addr: VAddr) -> bool {
        addr.is_user() && !self.areas.contains(addr)
    }

    fn check_overlapping_range(&self, range: VRange) -> bool {
        range.is_user() && !self.areas.contains_range(&range)
    }

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

        loop {
            let end = hint + len;

            if !VRange::new(hint, end).is_user() {
                return None;
            }

            let Some(ub) = self.areas.upper_bound(end) else {
                return Some(hint);
            };

            let ub_range = ub.range.as_ref(&self.lock);
            let ub_end = ub_range.end().ceil();
            if ub_end <= hint {
                return Some(hint);
            }

            hint = ub_end;
        }
    }

    async fn unmap(&mut self, start: VAddr, len: usize) -> KResult<Vec<Folio>> {
        assert_eq!(start.floor(), start);
        let end = (start + len).ceil();
        let range_to_unmap = VRange::new(start, end);
        if !range_to_unmap.is_user() {
            return Err(EINVAL);
        }

        // TODO: Write back dirty pages.
        let mut pages_to_free = Vec::new();

        let isolated_areas =
            self.areas.isolate(&range_to_unmap, &mut self.lock).await;

        for area in isolated_areas {
            let range = area.range.as_ref(&self.lock);

            for pte in self.page_table.iter_user(*range) {
                let Some(folio) = pte.take_if_present() else {
                    continue;
                };

                pages_to_free.push(folio);
            }
        }

        Ok(pages_to_free)
    }

    async fn protect(
        &mut self, start: VAddr, len: usize, permission: Permission,
    ) -> KResult<()> {
        assert_eq!(start.floor(), start);
        assert!(len != 0);

        let end = (start + len).ceil();
        let range_to_protect = VRange::new(start, end);
        if !range_to_protect.is_user() {
            return Err(EINVAL);
        }

        let isolated_areas =
            self.areas.isolate(&range_to_protect, &mut self.lock).await;

        let mut found = false;
        for area in isolated_areas {
            let range = area.range.as_ref(&self.lock);
            found = true;

            for pte in self.page_table.iter_user(*range) {
                let mut page_attr = pte
                    .get_attr()
                    .as_page_attr()
                    .expect("Not a page attribute");

                if !page_attr.contains(PageAttribute::PRESENT) {
                    // Skip PTEs that are not installed yet.
                    continue;
                }

                if !permission.read && !permission.write && !permission.execute
                {
                    // If no permissions are set, we just remove the page.
                    pte.release();
                    continue;
                }

                page_attr.set(PageAttribute::READ, permission.read);

                if !page_attr.contains(PageAttribute::COPY_ON_WRITE) {
                    page_attr.set(PageAttribute::WRITE, permission.write);
                }

                page_attr.set(PageAttribute::EXECUTE, permission.execute);

                pte.set_attr(page_attr.into());
            }

            area.set_permission(permission);

            // Insert it back.
            self.areas.insert(area);
        }

        if !found {
            return Err(ENOMEM);
        }

        Ok(())
    }

    fn mmap(
        &mut self, at: VAddr, len: usize, mapping: Mapping,
        permission: Permission, is_shared: bool,
    ) -> KResult<()> {
        assert_eq!(at.floor(), at);
        assert_eq!(len & (PAGE_SIZE - 1), 0);
        let range = VRange::new(at, at + len);

        // We are doing a area marker insertion.
        if len == 0 && !self.check_overlapping_addr(at)
            || !self.check_overlapping_range(range)
        {
            return Err(EEXIST);
        }

        self.areas.insert(MemArea::new(
            range,
            AreaFlags::from_old(permission, is_shared),
            mapping,
        ));

        Ok(())
    }
}

impl Drop for MMListInner {
    fn drop(&mut self) {
        // May buggy
        for area in self.areas.iter() {
            let range = area.range.as_ref(&self.lock).clone();

            if area.is_shared() {
                unimplemented!("Shared mapping is not yet implemented");
            }

            for pte in self.page_table.iter_user(range) {
                pte.release();
            }
        }

        // TODO: Recycle all pages in the page table.
    }
}

impl MMList {
    async fn flush_user_tlbs(&self) {
        match self.user_count.load(Ordering::Relaxed) {
            0 => {
                // If there are currently no users, we don't need to do anything.
            }
            1 => {
                if PAddr::from(get_root_page_table_pfn()).addr()
                    == self.root_page_table.load(Ordering::Relaxed)
                {
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

    #[inline(always)]
    fn _new(
        areas: AreaList, page_table: KernelPageTable, prog_break: ProgramBreak,
    ) -> Self {
        Self {
            root_page_table: AtomicUsize::from(page_table.addr().addr()),
            user_count: AtomicUsize::new(0),
            inner: ArcSwap::new(Mutex::new(MMListInner {
                lock: MemListLock::_new(),
                areas,
                page_table,
                prog_break,
            })),
        }
    }

    pub fn new() -> Self {
        Self::_new(
            AreaList::new(),
            KernelPageTable::new(),
            ProgramBreak::null(),
        )
    }

    pub async fn new_cloned(&self) -> Self {
        let inner = self.inner.borrow();
        let inner = inner.lock().await;

        let mut new_areas = AreaList::new();
        let new_pgtable = KernelPageTable::new();

        for area in inner.areas.iter() {
            dup_area_to_list(
                area,
                &mut new_areas,
                &inner.lock,
                &inner.page_table,
                &new_pgtable,
            );
        }

        // We've set some pages as CoW, so we need to invalidate all our users' TLB.
        self.flush_user_tlbs().await;

        Self::_new(new_areas, new_pgtable, inner.prog_break.clone())
    }

    pub async fn new_shared(&self) -> Self {
        todo!()
    }

    pub fn activate(&self) {
        self.user_count.fetch_add(1, Ordering::Acquire);

        let root_page_table = self.root_page_table.load(Ordering::Relaxed);
        assert_ne!(root_page_table, 0);
        set_root_page_table_pfn(PFN::from(PAddr::from(root_page_table)));
    }

    pub fn deactivate(&self) {
        set_root_page_table_pfn(PFN::from(GLOBAL_PAGE_TABLE.start()));

        let old_user_count = self.user_count.fetch_sub(1, Ordering::Release);
        assert_ne!(old_user_count, 0);
    }

    /// Replace the current page table with a new one.
    ///
    /// # Safety
    /// This function should be called only when we are sure that the `MMList` is not
    /// being used by any other thread.
    pub unsafe fn replace(&self, new: Option<Self>) {
        eonix_preempt::disable();

        assert_eq!(
            self.user_count.load(Ordering::Relaxed),
            1,
            "We should be the only user"
        );

        assert_eq!(
            new.as_ref()
                .map(|new_mm| new_mm.user_count.load(Ordering::Relaxed))
                .unwrap_or(0),
            0,
            "`new` must not be used by anyone"
        );

        let old_root_page_table = self.root_page_table.load(Ordering::Relaxed);
        let current_root_page_table = get_root_page_table_pfn();
        assert_eq!(
            PAddr::from(current_root_page_table).addr(),
            old_root_page_table,
            "We should be the only user"
        );

        let new_root_page_table = match &new {
            Some(new_mm) => new_mm.root_page_table.load(Ordering::Relaxed),
            None => GLOBAL_PAGE_TABLE.start().addr(),
        };

        set_root_page_table_pfn(PFN::from(PAddr::from(new_root_page_table)));

        self.root_page_table
            .store(new_root_page_table, Ordering::Relaxed);

        // TODO: Check whether we should wake someone up if they've been put
        //       to sleep when calling `vfork`.
        let old_mm = self
            .inner
            .swap(new.map(|new_mm| new_mm.inner.swap(None)).flatten());

        eonix_preempt::enable();

        // This could take long...
        drop(old_mm);
    }

    pub fn release(&self) {
        let old_mm = self.inner.swap(None);
        let old_table = self.root_page_table.swap(0, Ordering::Relaxed);

        // TODO: Remove this completely...
        // XXX: `ArcSwap` is broken and never safe to use. Check `replace` above.
        assert_ne!(old_table, 0, "Already released?");
        assert!(old_mm.is_some(), "Already released?");
    }

    /// No need to do invalidation manually, `PageTable` already does it.
    pub async fn unmap(&self, start: VAddr, len: usize) -> KResult<()> {
        let pages_to_free = {
            let inner = self.inner.borrow();
            let mut inner = inner.lock().await;

            inner.unmap(start, len).await?
        };

        // We need to assure that the pages are not accessed anymore.
        // The ones having these pages in their TLB could read from or write to them.
        // So flush the TLBs first for all our users.
        self.flush_user_tlbs().await;

        // Then free the pages.
        drop(pages_to_free);

        Ok(())
    }

    pub async fn protect(
        &self, start: VAddr, len: usize, prot: Permission,
    ) -> KResult<()> {
        let inner = self.inner.borrow();
        let mut inner = inner.lock().await;
        inner.protect(start, len, prot).await?;

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
        let inner = inner.lock().await;

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
        &self, hint: VAddr, len: usize, mapping: Mapping,
        permission: Permission, is_shared: bool,
    ) -> KResult<VAddr> {
        let inner = self.inner.borrow();
        let mut inner = inner.lock().await;

        if hint == VAddr::NULL {
            let at = inner.find_available(hint, len).ok_or(ENOMEM)?;
            inner.mmap(at, len, mapping, permission, is_shared)?;
            return Ok(at);
        }

        match inner.mmap(hint, len, mapping.clone(), permission, is_shared) {
            Ok(()) => Ok(hint),
            Err(EEXIST) => {
                let at = inner.find_available(hint, len).ok_or(ENOMEM)?;
                inner.mmap(at, len, mapping, permission, is_shared)?;
                Ok(at)
            }
            Err(err) => Err(err),
        }
    }

    pub async fn mmap_fixed(
        &self, at: VAddr, len: usize, mapping: Mapping, permission: Permission,
        is_shared: bool,
    ) -> KResult<VAddr> {
        self.inner
            .borrow()
            .lock()
            .await
            .mmap(at, len, mapping.clone(), permission, is_shared)
            .map(|_| at)
    }

    /// Access the memory area with the given function.
    /// The function will be called with the offset of the area and the slice of the area.
    pub async fn access_mut<F>(
        &self, start: VAddr, len: usize, func: F,
    ) -> KResult<()>
    where
        F: Fn(usize, &mut [u8]),
    {
        // First, validate the address range.
        let end = start + len;
        if !start.is_user() || !end.is_user() {
            return Err(EINVAL);
        }

        let inner = self.inner.borrow();
        let inner = inner.lock().await;

        let mut offset = 0;
        let mut remaining = len;
        let mut current = start;

        while remaining > 0 {
            let area = inner.overlapping_addr(current).ok_or(EFAULT)?;

            let range = area.range.as_ref(&inner.lock);
            let area_start = range.start();
            let area_end = range.end();
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
                    Folio::with_raw(pte.get_pfn(), |page| {
                        let mut pg = page.lock();
                        let page_data =
                            &mut pg.as_bytes_mut()[start_offset..end_offset];

                        func(offset + idx * 0x1000, page_data);
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
    fn set_copy_on_write(&self, from: &Self, range: VRange);
}

impl PageTableExt for KernelPageTable {
    fn set_copy_on_write(&self, from: &Self, range: VRange) {
        let to_iter = self.iter_user(range);
        let from_iter = from.iter_user(range);

        for (to, from) in to_iter.zip(from_iter) {
            to.set_copy_on_write(from);
        }
    }
}

trait PTEExt {
    fn set_copy_on_write(&mut self, from: &mut Self);
    fn take_if_present(&mut self) -> Option<Folio>;
    fn release(&mut self);
}

impl<T> PTEExt for T
where
    T: PTE,
{
    fn set_copy_on_write(&mut self, from: &mut Self) {
        let (pfn, raw_attr) = from.get();
        let mut attr = raw_attr.as_page_attr().expect("Not a page attribute");

        if !attr.contains(PageAttribute::PRESENT) {
            // Copy non-installed mapped PTEs directly to the new PTE and delay
            // its handling till the page fault.
            let (pfn, attr) = from.get();
            self.set(pfn, attr);
            return;
        }

        attr.remove(PageAttribute::WRITE | PageAttribute::DIRTY);
        attr.insert(PageAttribute::COPY_ON_WRITE);

        let pfn = unsafe {
            // SAFETY: We get the pfn from a valid page table entry, which
            //         should have been created via `add_mapping`.
            duplicate_mapping(pfn)
        };

        self.set(pfn, T::Attr::from(attr & !PageAttribute::ACCESSED));
        from.set_attr(T::Attr::from(attr));
    }

    fn take_if_present(&mut self) -> Option<Folio> {
        let (pfn, raw_attr) = self.take();
        let attr = raw_attr.as_page_attr().expect("Not a page");

        attr.contains(PageAttribute::PRESENT).then(|| unsafe {
            // SAFETY: Present PTEs are always created via `add_mapping`.
            remove_mapping(pfn)
        })
    }

    fn release(&mut self) {
        // Drop the returned folio.
        let _ = self.take_if_present();
    }
}
