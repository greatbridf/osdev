use lazy_static::lazy_static;

use crate::prelude::*;

use crate::bindings::root::{EINVAL, KERNEL_PML4};

use super::{
    paging::Page,
    phys::{CachedPP, PhysPtr as _},
    VAddr, VRange,
};
use super::{MMArea, Permission};

const PA_P: usize = 0x001;
const PA_RW: usize = 0x002;
const PA_US: usize = 0x004;
const PA_PWT: usize = 0x008;
const PA_PCD: usize = 0x010;
const PA_A: usize = 0x020;
const PA_D: usize = 0x040;
const PA_PS: usize = 0x080;
const PA_G: usize = 0x100;
const PA_COW: usize = 0x200;
const PA_MMAP: usize = 0x400;
const PA_ANON: usize = 0x800;
const PA_NXE: usize = 0x8000_0000_0000_0000;
const PA_MASK: usize = 0xfff0_0000_0000_0fff;

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct PTE(usize);

#[derive(Debug)]
pub struct PageTable {
    page: Page,
}

pub struct PTEIterator<'lt, const KERNEL: bool> {
    count: usize,
    i4: u16,
    i3: u16,
    i2: u16,
    i1: u16,
    p4: CachedPP,
    p3: CachedPP,
    p2: CachedPP,
    p1: CachedPP,

    start: VAddr,
    end: VAddr,
    _phantom: core::marker::PhantomData<&'lt ()>,
}

lazy_static! {
    static ref EMPTY_PAGE: Page = {
        let page = Page::alloc_one();
        page.zero();
        page
    };
}

impl PTE {
    pub fn is_user(&self) -> bool {
        self.0 & PA_US != 0
    }

    pub fn is_present(&self) -> bool {
        self.0 & PA_P != 0
    }

    pub fn pfn(&self) -> usize {
        self.0 & !PA_MASK
    }

    pub fn attributes(&self) -> usize {
        self.0 & PA_MASK
    }

    pub fn set(&mut self, pfn: usize, attributes: usize) {
        self.0 = pfn | attributes;
    }

    pub fn set_pfn(&mut self, pfn: usize) {
        self.set(pfn, self.attributes())
    }

    pub fn set_attributes(&mut self, attributes: usize) {
        self.set(self.pfn(), attributes)
    }

    fn parse_page_table(&mut self, kernel: bool) -> CachedPP {
        let attributes = if kernel {
            PA_P | PA_RW | PA_G
        } else {
            PA_P | PA_RW | PA_US
        };

        if self.is_present() {
            CachedPP::new(self.pfn())
        } else {
            let page = Page::alloc_one();
            let pp = page.as_cached();
            page.zero();

            self.set(page.into_pfn(), attributes);
            pp
        }
    }

    pub fn setup_cow(&mut self, from: &mut Self) {
        self.set(
            unsafe { Page::from_pfn(from.pfn(), 0) }.into_pfn(),
            (from.attributes() & !(PA_RW | PA_A | PA_D)) | PA_COW,
        );

        from.set_attributes((from.attributes() & !PA_RW) | PA_COW);
    }

    pub fn clear(&mut self) {
        self.set(0, 0)
    }

    /// Take the ownership of the page from the PTE, clear the PTE and return the page.
    pub fn take(&mut self) -> Page {
        // SAFETY: Acquire the ownership of the page from the page table and then
        // clear the PTE so no one could be able to access the page from here later on.
        let page = unsafe { Page::take_pfn(self.pfn(), 0) };
        self.clear();
        page
    }
}

impl<'lt, const KERNEL: bool> PTEIterator<'lt, KERNEL> {
    fn new(pt: &'lt Page, start: VAddr, end: VAddr) -> KResult<Self> {
        if start > end {
            return Err(EINVAL);
        }

        let p4 = pt.as_cached();
        let p3 = p4.as_mut_slice::<PTE>(512)[Self::index(4, start)].parse_page_table(KERNEL);
        let p2 = p3.as_mut_slice::<PTE>(512)[Self::index(3, start)].parse_page_table(KERNEL);
        let p1 = p2.as_mut_slice::<PTE>(512)[Self::index(2, start)].parse_page_table(KERNEL);

        Ok(Self {
            count: (end.0 - start.0) >> 12,
            i4: Self::index(4, start) as u16,
            i3: Self::index(3, start) as u16,
            i2: Self::index(2, start) as u16,
            i1: Self::index(1, start) as u16,
            p4,
            p3,
            p2,
            p1,
            start,
            end,
            _phantom: core::marker::PhantomData,
        })
    }

    fn offset(level: u32) -> usize {
        12 + (level as usize - 1) * 9
    }

    fn index(level: u32, vaddr: VAddr) -> usize {
        (vaddr.0 >> Self::offset(level)) & 0x1ff
    }
}

impl<'lt, const KERNEL: bool> Iterator for PTEIterator<'lt, KERNEL> {
    type Item = &'lt mut PTE;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count != 0 {
            self.count -= 1;
        } else {
            return None;
        }

        let retval = &mut self.p1.as_mut_slice::<PTE>(512)[self.i1 as usize];
        self.i1 = (self.i1 + 1) % 512;
        if self.i1 == 0 {
            self.i2 = (self.i2 + 1) % 512;
            if self.i2 == 0 {
                self.i3 = (self.i3 + 1) % 512;
                if self.i3 == 0 {
                    self.i4 = (self.i4 + 1) % 512;
                    if self.i4 == 0 {
                        panic!("PTEIterator: out of range");
                    }
                }
                self.p3 =
                    self.p4.as_mut_slice::<PTE>(512)[self.i4 as usize].parse_page_table(KERNEL);
            }
            self.p2 = self.p3.as_mut_slice::<PTE>(512)[self.i3 as usize].parse_page_table(KERNEL);
        }
        self.p1 = self.p2.as_mut_slice::<PTE>(512)[self.i2 as usize].parse_page_table(KERNEL);
        Some(retval)
    }
}

impl PageTable {
    pub fn new() -> Self {
        let page = Page::alloc_one();
        page.zero();

        // TODO: copy only the kernel space mappings.
        let kernel_space_page_table = CachedPP::new(KERNEL_PML4 as usize);

        page.as_cached().as_mut_slice::<u64>(512)[256..]
            .copy_from_slice(&kernel_space_page_table.as_mut_slice(512)[256..]);

        Self { page }
    }

    pub fn iter_user(&self, range: VRange) -> KResult<PTEIterator<'_, false>> {
        PTEIterator::new(&self.page, range.start().floor(), range.end().ceil())
    }

    pub fn iter_kernel(&self, range: VRange) -> KResult<PTEIterator<'_, true>> {
        PTEIterator::new(&self.page, range.start().floor(), range.end().ceil())
    }

    pub fn switch(&self) {
        arch::set_root_page_table(self.page.as_phys())
    }

    pub fn unmap(&self, area: &MMArea) {
        let range = area.range();
        let use_invlpg = range.len() / 4096 < 4;
        let iter = self.iter_user(range).unwrap();

        if self.page.as_phys() != arch::get_root_page_table() {
            for pte in iter {
                pte.take();
            }
            return;
        }

        if use_invlpg {
            for (offset_pages, pte) in iter.enumerate() {
                pte.take();

                let pfn = range.start().floor().0 + offset_pages * 4096;
                arch::flush_tlb(pfn);
            }
        } else {
            for pte in iter {
                pte.take();
            }
            arch::flush_tlb_all();
        }
    }

    pub fn lazy_invalidate_tlb_all(&self) {
        if self.page.as_phys() == arch::get_root_page_table() {
            arch::flush_tlb_all();
        }
    }

    pub fn set_mmapped(&self, range: VRange, permission: Permission) {
        // PA_RW is set during page fault handling.
        // PA_NXE is preserved across page faults, so we set PA_NXE now.
        let attributes = if permission.execute {
            PA_US | PA_COW | PA_ANON | PA_MMAP
        } else {
            PA_US | PA_COW | PA_ANON | PA_MMAP | PA_NXE
        };

        for pte in self.iter_user(range).unwrap() {
            pte.set(EMPTY_PAGE.clone().into_pfn(), attributes);
        }
    }

    pub fn set_anonymous(&self, range: VRange, permission: Permission) {
        // PA_RW is set during page fault handling.
        // PA_NXE is preserved across page faults, so we set PA_NXE now.
        let attributes = if permission.execute {
            PA_P | PA_US | PA_COW | PA_ANON
        } else {
            PA_P | PA_US | PA_COW | PA_ANON | PA_NXE
        };

        for pte in self.iter_user(range).unwrap() {
            pte.set(EMPTY_PAGE.clone().into_pfn(), attributes);
        }
    }
}

fn drop_page_table_recursive(pt: &Page, level: usize) {
    for pte in pt
        .as_cached()
        .as_mut_slice::<PTE>(512)
        .iter_mut()
        .filter(|pte| pte.is_present() && pte.is_user())
    {
        let page = pte.take();
        if level > 1 {
            drop_page_table_recursive(&page, level - 1);
        }
    }
}

impl Drop for PageTable {
    fn drop(&mut self) {
        drop_page_table_recursive(&self.page, 4);
    }
}
