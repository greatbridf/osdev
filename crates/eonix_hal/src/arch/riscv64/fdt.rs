extern crate alloc;

use alloc::string::String;

use fdt::Fdt;
use spin::Once;

static GLOBAL_FDT: Once<Fdt<'static>> = Once::new();

pub unsafe fn init_dtb_and_fdt(dtb_pa: usize) {
    let dtb_virt_ptr = dtb_pa as *const u8;

    GLOBAL_FDT.call_once(|| {
        unsafe { Fdt::from_ptr(dtb_virt_ptr)
            .expect("Failed to parse DTB from static memory.") }
    });
}

pub fn get_global_fdt() -> &'static Fdt<'static> {
    GLOBAL_FDT.get().expect("Global FDT has not been initialized yet")
}

#[derive(Debug, Clone)]
pub struct HartInfo {
    pub hart_id: usize,
    pub compatible: Option<String>,
}

pub fn get_num_harts() -> usize {
    let fdt = get_global_fdt();
    let dtb_cpus = fdt.cpus();
    let mut num_harts = 0;

    for _cpu in dtb_cpus {
        num_harts += 1;
    }
    num_harts
}
