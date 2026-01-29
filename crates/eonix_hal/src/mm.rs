use core::alloc::{AllocError, Allocator, Layout};
use core::cell::RefCell;
use core::ptr::NonNull;

use eonix_mm::address::{AddrOps as _, PRange};
use eonix_mm::page_table::PageTableAlloc;
use eonix_mm::paging::{BasicFolio, FrameAlloc, PAGE_SIZE, PFN};

pub use crate::arch::mm::{
    flush_tlb, flush_tlb_all, get_root_page_table_pfn, set_root_page_table_pfn,
    ArchMemory, ArchPhysAccess, GLOBAL_PAGE_TABLE,
};

pub struct BasicPageAlloc {
    ranges: [Option<PRange>; Self::MAX],
    head: usize,
    tail: usize,
}

struct ScopedAllocInner<'a> {
    _memory: &'a mut [u8],
    current: NonNull<[u8]>,
    allocated_count: usize,
}

pub struct ScopedAllocator<'a> {
    inner: RefCell<ScopedAllocInner<'a>>,
}

impl BasicPageAlloc {
    const MAX: usize = 32;

    pub const fn new() -> Self {
        Self {
            ranges: [None; Self::MAX],
            head: 0,
            tail: 0,
        }
    }

    fn alloc_one(&mut self) -> PFN {
        assert_ne!(self.head, self.tail, "No free pages available");
        let mut range = self.ranges[self.head].take().unwrap();
        let pfn = PFN::from(range.start());
        range = PRange::new(range.start() + PAGE_SIZE, range.end());

        if range.len() != 0 {
            self.ranges[self.head] = Some(range);
        } else {
            self.head += 1;
            self.head %= Self::MAX;
        }

        pfn
    }

    fn alloc_order(&mut self, order: u32) -> PFN {
        assert!(order <= 4);
        let me = core::mem::replace(self, Self::new());

        let mut found = None;
        for mut range in me.into_iter() {
            if found.is_some() || range.len() < (PAGE_SIZE << order) {
                self.add_range(range);
                continue;
            }

            range = range.shrink(PAGE_SIZE << order);
            found = Some(PFN::from(range.end()));

            if range.len() != 0 {
                self.add_range(range);
            }
        }

        found.expect("No free pages available for the requested order")
    }

    pub fn add_range(&mut self, range: PRange) {
        let tail = self.tail;

        self.tail += 1;
        self.tail %= Self::MAX;

        if self.tail == self.head {
            panic!("Page allocator is full");
        }

        self.ranges[tail] =
            Some(PRange::new(range.start().ceil(), range.end().floor()));
    }

    pub fn alloc(&mut self, order: u32) -> PFN {
        match order {
            0 => self.alloc_one(),
            ..=4 => self.alloc_order(order),
            _ => panic!("Order {} is too large for BasicPageAlloc", order),
        }
    }

    pub fn into_iter(self) -> impl Iterator<Item = PRange> {
        self.ranges
            .into_iter()
            .cycle()
            .skip(self.head)
            .map_while(|x| x)
    }
}

#[derive(Clone)]
pub struct BasicPageAllocRef<'a>(&'a RefCell<BasicPageAlloc>);

impl<'a> BasicPageAllocRef<'a> {
    pub const fn new(alloc: &'a RefCell<BasicPageAlloc>) -> Self {
        Self(alloc)
    }
}

impl FrameAlloc for BasicPageAllocRef<'_> {
    type Folio = BasicFolio;

    fn alloc_order(&self, order: u32) -> Option<Self::Folio> {
        Some(BasicFolio::new(self.0.borrow_mut().alloc(order), order))
    }
}

impl PageTableAlloc for BasicPageAllocRef<'_> {
    type Folio = BasicFolio;

    fn alloc(&self) -> Self::Folio {
        FrameAlloc::alloc(self).unwrap()
    }

    unsafe fn from_raw(&self, pfn: PFN) -> Self::Folio {
        BasicFolio::new(pfn, 0)
    }
}

impl<'a> ScopedAllocator<'a> {
    pub fn new(memory: &'a mut [u8]) -> Self {
        ScopedAllocator {
            inner: RefCell::new(ScopedAllocInner {
                current: NonNull::new(memory).unwrap(),
                _memory: memory,
                allocated_count: 0,
            }),
        }
    }

    pub fn with_alloc<'b, 'r, O>(
        &'r self,
        func: impl FnOnce(&'b ScopedAllocator<'a>) -> O,
    ) -> O
    where
        'a: 'b,
        'r: 'b,
    {
        func(self)
    }
}

unsafe impl Allocator for &ScopedAllocator<'_> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let mut inner = self.inner.borrow_mut();
        let memory = &mut inner.current;

        let addr: NonNull<u8> = memory.cast();
        let offset = addr.align_offset(layout.align());

        if offset + layout.size() > memory.len() {
            return Err(AllocError);
        }

        let allocated = unsafe {
            // SAFETY: `addr + offset` won't overflow.
            NonNull::slice_from_raw_parts(addr.add(offset), layout.size())
        };

        unsafe {
            // SAFETY: `allocated + layout.size()` won't overflow.
            *memory = NonNull::slice_from_raw_parts(
                allocated.cast::<u8>().add(layout.size()),
                memory.len() - offset - layout.size(),
            );
        }

        inner.allocated_count += 1;
        Ok(allocated)
    }

    unsafe fn deallocate(&self, _: NonNull<u8>, _: Layout) {
        self.inner.borrow_mut().allocated_count -= 1;
    }
}

impl Drop for ScopedAllocator<'_> {
    fn drop(&mut self) {
        let inner = self.inner.borrow();
        if inner.allocated_count > 0 {
            panic!(
                "Memory leak detected: {} allocations not deallocated",
                inner.allocated_count
            );
        }
    }
}
