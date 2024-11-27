pub unsafe fn save_percpu_pointer(percpu_area_base: *mut ()) {
    #[cfg(target_arch = "x86_64")]
    x86_64::task::wrmsr(0xC0000101, percpu_area_base as u64);

    #[cfg(not(target_arch = "x86_64"))]
    compile_error!("unsupported architecture");
}

pub unsafe fn set_percpu_area_thiscpu(percpu_area_base: *mut ()) {
    use core::arch::asm;

    save_percpu_pointer(percpu_area_base);

    #[cfg(target_arch = "x86_64")]
    {
        asm!(
            "movq {}, %gs:0",
            in(reg) percpu_area_base,
            options(att_syntax)
        );
    }

    #[cfg(not(target_arch = "x86_64"))]
    compile_error!("unsupported architecture");
}
