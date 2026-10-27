use core::ops::Deref;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use eonix_mm::address::{Addr, AddrOps, PAddr, PRange, PhysAccess};
use eonix_sync_base::LazyLock;
use fdt::Fdt;

use super::mm::ArchPhysAccess;
use crate::arch::riscv64::config::mm::KIMAGE_OFFSET;
use crate::extern_symbol_addr;

static DTB_PADDR: AtomicUsize = AtomicUsize::new(0);
pub static FDT: LazyLock<FdtExt> = LazyLock::new(|| unsafe {
    FdtExt::new(PAddr::from_val(DTB_PADDR.load(Ordering::Relaxed)))
});

pub struct FdtExt {
    fdt: Fdt<'static>,
    range: PRange,
}

impl FdtExt {
    /// # Safety
    /// The caller MUST ensure that [`addr`] points to valid FDT.
    pub unsafe fn new(addr: PAddr) -> Self {
        let fdt = unsafe {
            Fdt::from_ptr(ArchPhysAccess::as_ptr(addr).as_ptr())
                .expect("Failed to parse DTB from static memory.")
        };

        Self {
            range: PRange::from(addr).grow(fdt.total_size()),
            fdt,
        }
    }

    pub fn harts(&self) -> impl Iterator<Item = usize> {
        self.cpus().map(|cpu| cpu.ids().all()).flatten()
    }

    pub fn hart_count(&self) -> usize {
        self.harts().count()
    }

    pub fn present_ram(&self) -> impl Iterator<Item = PRange> {
        let mut index = 0;

        core::iter::from_fn(move || {
            let item = self
                .memory()
                .regions()
                .filter_map(|region| {
                    let start = PAddr::from(region.starting_address as usize);
                    Some(start).zip(region.size)
                })
                .map(|(start, len)| PRange::from(start).grow(len))
                .nth(index);

            index += 1;
            item
        })
    }

    pub fn free_ram(&self) -> impl Iterator<Item = PRange> {
        let kernel_end = extern_symbol_addr!(__kernel_end) - KIMAGE_OFFSET;
        let kernel_end = PAddr::from(kernel_end).ceil();

        // TODO: move this to some platform-specific crate
        self.present_ram().map(move |mut range| {
            // Strip out parts before __kernel_end
            if range.overlap_with(&PRange::from(kernel_end)) {
                (_, range) = range.split_at(kernel_end);
            }

            // Strip out part after the FDT
            if range.overlap_with(&self.range) {
                (range, _) = range.split_at(self.range.start());
            }

            range
        })
    }
}

impl Deref for FdtExt {
    type Target = Fdt<'static>;

    fn deref(&self) -> &Self::Target {
        &self.fdt
    }
}

pub unsafe fn init_dtb_and_fdt(dtb_paddr: PAddr) {
    DTB_PADDR.store(dtb_paddr.addr(), Ordering::Relaxed);
}
