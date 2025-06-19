use super::mm::{ArchPhysAccess, PresentRam};
use crate::arch::riscv64::config::mm::KIMAGE_OFFSET;
use core::sync::atomic::{AtomicPtr, Ordering};
use eonix_mm::address::{PAddr, PRange, PhysAccess};
use eonix_sync_base::LazyLock;
use fdt::Fdt;

static DTB_VIRT_PTR: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
pub static FDT: LazyLock<Fdt<'static>> = LazyLock::new(|| unsafe {
    Fdt::from_ptr(DTB_VIRT_PTR.load(Ordering::Acquire))
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

    fn present_ram(&self) -> impl Iterator<Item = PRange> + PresentRam {
        struct Present<I>(I);
        impl<I> PresentRam for Present<I> where I: Iterator<Item = PRange> {}
        impl<I> Iterator for Present<I>
        where
            I: Iterator<Item = PRange>,
        {
            type Item = PRange;

            fn next(&mut self) -> Option<Self::Item> {
                self.0.next()
            }
        }

        let mut index = 0;
        Present(core::iter::from_fn(move || {
            self.memory()
                .regions()
                .filter_map(|region| {
                    region.size.map(|len| {
                        PRange::from(PAddr::from(region.starting_address as usize)).grow(len)
                    })
                })
                .skip(index)
                .next()
                .inspect(|_| index += 1)
        }))
    }
}

pub unsafe fn init_dtb_and_fdt(dtb_paddr: PAddr) {
    let dtb_virt_ptr = ArchPhysAccess::as_ptr(dtb_paddr);
    DTB_VIRT_PTR.store(dtb_virt_ptr.as_ptr(), Ordering::Release);
}
