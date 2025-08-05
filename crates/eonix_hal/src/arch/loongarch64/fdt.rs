use super::mm::ArchPhysAccess;
use core::sync::atomic::{AtomicPtr, Ordering};
use eonix_mm::address::{Addr, PAddr, PRange, PhysAccess};
use eonix_sync_base::LazyLock;
use fdt::Fdt;

const FDT_PADDR: PAddr = PAddr::from_val(0x100000);

pub static FDT: LazyLock<Fdt<'static>> = LazyLock::new(|| unsafe {
    Fdt::from_ptr(ArchPhysAccess::as_ptr(FDT_PADDR).as_ptr())
        .expect("Failed to parse DTB from static memory.")
});

pub trait FdtExt {
    fn harts(&self) -> impl Iterator<Item = usize>;

    fn hart_count(&self) -> usize {
        self.harts().count()
    }

    fn present_ram(&self) -> impl Iterator<Item = PRange>;
}

impl FdtExt for Fdt<'_> {
    fn harts(&self) -> impl Iterator<Item = usize> {
        self.cpus().map(|cpu| cpu.ids().all()).flatten()
    }

    fn present_ram(&self) -> impl Iterator<Item = PRange> {
        core::iter::empty()
    }
}
