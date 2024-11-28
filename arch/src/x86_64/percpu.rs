use super::wrmsr;
use core::{arch::asm, ptr::NonNull};

fn save_percpu_pointer(percpu_area_base: NonNull<u8>) {
    wrmsr(0xC0000101, percpu_area_base.as_ptr() as u64);
}

pub unsafe fn init_percpu_area_thiscpu(percpu_area_base: NonNull<u8>) {
    save_percpu_pointer(percpu_area_base);

    asm!(
        "movq {}, %gs:0",
        in(reg) percpu_area_base.as_ptr(),
        options(att_syntax)
    );
}
