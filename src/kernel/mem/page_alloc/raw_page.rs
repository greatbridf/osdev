use core::marker::PhantomData;
use core::mem::{offset_of, MaybeUninit};
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use buddy_allocator::BuddyFolio;
use eonix_hal::mm::ArchPhysAccess;
use eonix_mm::address::{PAddr, PhysAccess as _};
use eonix_mm::paging::{FolioList, FolioListSized, Zone, PFN};
use intrusive_list::{container_of, Link, List};
use slab_allocator::{SlabPage, SlabPageAlloc, SlabSlot};

use super::zones::ZONE;
use super::{GlobalPageAlloc, PerCpuPage};
use crate::kernel::mem::PhysAccess;

pub struct PageFlags(AtomicU32);

#[derive(Clone, Copy)]
pub struct SlabPageExt {
    allocated_count: usize,
    free_next: Option<NonNull<SlabSlot>>,
}

impl SlabPageExt {
    const fn new() -> Self {
        Self {
            allocated_count: 0,
            free_next: None,
        }
    }
}

/// Extention data of Pages.
///
/// # Safety
/// Implementors MUST guarantee that the type is less that 32 bytes.
pub unsafe trait PageExt {}

pub struct AnyExt();

#[allow(unused)]
pub struct RealExt([u8; 32]);

unsafe impl PageExt for AnyExt {}
unsafe impl PageExt for RealExt {}
unsafe impl PageExt for SlabPageExt {}

/// The real struct Page in memory, big enough to accomodate Page +
/// 32 bytes of extention data.
pub type PageReal = Page<RealExt>;

pub type PageSlab = Page<SlabPageExt>;

pub struct Page<E = AnyExt>
where
    E: PageExt + ?Sized,
{
    /// This can be used for LRU page swap in the future.
    ///
    /// Now only used for free page links in the buddy system.
    pub link: Link,
    /// # Safety
    /// This field is only used in buddy system and is protected by the global lock.
    pub order: u32,
    pub flags: PageFlags,
    pub refcount: AtomicUsize,

    data: E,
}

impl PageFlags {
    pub const LOCKED: u32 = 1 << 1;
    pub const BUDDY: u32 = 1 << 2;
    pub const SLAB: u32 = 1 << 3;
    pub const DIRTY: u32 = 1 << 4;
    pub const LOCAL: u32 = 1 << 6;

    pub fn has(&self, flag: u32) -> bool {
        (self.0.load(Ordering::Relaxed) & flag) == flag
    }

    pub fn set(&self, flag: u32) {
        self.0.fetch_or(flag, Ordering::Relaxed);
    }

    pub fn clear(&self, flag: u32) {
        self.0.fetch_and(!flag, Ordering::Relaxed);
    }

    /// Set the flag and return whether it was already set.
    ///
    /// If multiple flags are given, returns true if any of them were already set.
    pub fn test_and_set(&self, flag: u32) -> bool {
        (self.0.fetch_or(flag, Ordering::Relaxed) & flag) != 0
    }
}

impl<E> AsRef<Page> for Page<E>
where
    E: PageExt,
{
    fn as_ref(&self) -> &Page {
        unsafe {
            // SAFETY: T is ?Sized for Page<T>, so they have the same layout.
            &*(self as *const _ as *const Page)
        }
    }
}

impl Page {
    /// Cast the Page into specific type.
    ///
    /// # Safety
    /// The caller MUST guarantee that the page has the extension of type `E`.
    pub unsafe fn cast_unchecked<E>(&self) -> &Page<E>
    where
        E: PageExt,
    {
        // SAFETY: The caller guarantees that the page is a `Page<E>`.
        &*(self as *const _ as *const Page<E>)
    }

    /// Cast the Page into specific type.
    ///
    /// # Safety
    /// The caller MUST guarantee that the page has the extension of type `E`.
    pub unsafe fn cast_mut_unchecked<E>(&mut self) -> &mut Page<E>
    where
        E: PageExt,
    {
        // SAFETY: The caller guarantees that the page is a `Page<E>`.
        &mut *(self as *const _ as *mut Page<E>)
    }

    /// Cast the Page into specific type.
    ///
    /// # Safety
    /// The caller MUST guarantee that the page was not initialized.
    pub unsafe fn init_unchecked<E>(&mut self, ext: E) -> &mut Page<E>
    where
        E: PageExt,
    {
        let mut typed_ptr = NonNull::from_mut(self)
            .byte_add(offset_of!(Page<E>, data))
            .cast::<MaybeUninit<E>>();

        // SAFETY: The caller ensures that the page was not initialized.
        typed_ptr.as_mut().write(ext);

        self.cast_mut_unchecked()
    }
}

impl<E> Deref for Page<E>
where
    E: PageExt + ?Sized,
{
    type Target = E;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<E> DerefMut for Page<E>
where
    E: PageExt + ?Sized,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl BuddyFolio for Page {
    fn pfn(&self) -> PFN {
        ZONE.get_pfn(self)
    }

    fn get_order(&self) -> u32 {
        self.order
    }

    fn is_buddy(&self) -> bool {
        self.flags.has(PageFlags::BUDDY)
    }

    fn set_order(&mut self, order: u32) {
        self.order = order;
    }

    fn set_buddy(&mut self, val: bool) {
        if val {
            self.flags.set(PageFlags::BUDDY);
        } else {
            self.flags.clear(PageFlags::BUDDY)
        }
    }
}

impl SlabPage for PageSlab {
    fn get_data_ptr(&self) -> NonNull<[u8]> {
        let paddr_start = PAddr::from(ZONE.get_pfn(self.as_ref()));
        let page_data_ptr = unsafe { paddr_start.as_ptr() };

        NonNull::slice_from_raw_parts(page_data_ptr, 1 << (self.order + 12))
    }

    fn get_free_slot(&self) -> Option<NonNull<SlabSlot>> {
        self.free_next
    }

    fn set_free_slot(&mut self, next: Option<NonNull<SlabSlot>>) {
        self.free_next = next;
    }

    fn get_alloc_count(&self) -> usize {
        self.allocated_count
    }

    fn inc_alloc_count(&mut self) -> usize {
        self.allocated_count += 1;
        self.allocated_count
    }

    fn dec_alloc_count(&mut self) -> usize {
        self.allocated_count -= 1;
        self.allocated_count
    }

    unsafe fn from_allocated(ptr: NonNull<u8>) -> &'static mut Self {
        unsafe {
            // SAFETY: The caller ensures that `ptr` is valid.
            let paddr = ArchPhysAccess::from_ptr(ptr);
            let pfn = PFN::from(paddr);

            ZONE.get_page(pfn)
                .expect("Page outside of the global zone")
                .as_mut()
                .cast_mut_unchecked()
        }
    }
}

impl PerCpuPage for Page {
    fn set_local(&mut self, val: bool) {
        if val {
            self.flags.set(PageFlags::LOCAL)
        } else {
            self.flags.clear(PageFlags::LOCAL)
        }
    }
}

pub struct RawPageList<E = AnyExt>(List, PhantomData<E>);

unsafe impl<F> Send for RawPageList<F> {}

impl<E> FolioList for RawPageList<E>
where
    E: PageExt,
{
    type Folio = Page<E>;

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn peek_head(&mut self) -> Option<&mut Self::Folio> {
        unsafe {
            let link = self.0.head()?;
            let mut raw_page_ptr = container_of!(link, Page<E>, link);

            Some(raw_page_ptr.as_mut())
        }
    }

    fn pop_head(&mut self) -> Option<&'static mut Self::Folio> {
        unsafe {
            let link = self.0.pop()?;
            let mut raw_page_ptr = container_of!(link, Page<E>, link);

            Some(raw_page_ptr.as_mut())
        }
    }

    fn push_tail(&mut self, page: &'static mut Self::Folio) {
        self.0.insert(&mut page.link);
    }

    fn remove(&mut self, page: &mut Self::Folio) {
        self.0.remove(&mut page.link)
    }
}

impl<E> FolioListSized for RawPageList<E>
where
    E: PageExt,
{
    const NEW: Self = RawPageList(List::new(), PhantomData);
}

unsafe impl SlabPageAlloc for GlobalPageAlloc {
    type Page = PageSlab;
    type PageList = RawPageList<SlabPageExt>;

    fn alloc_slab_page(&self) -> &'static mut Self::Page {
        let raw_page = self.alloc_raw_order(0).expect("Out of memory");

        let page = unsafe { raw_page.init_unchecked(SlabPageExt::new()) };

        // XXX: Use Ordering::Release
        page.flags.set(PageFlags::SLAB);

        page
    }
}
