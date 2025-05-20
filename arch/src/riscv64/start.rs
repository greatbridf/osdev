extern "C" {
    fn kernel_init();
}

/// bootstrap in rust
#[no_mangle]
pub fn start() {
    // TODO: some init
    unsafe { kernel_init() };
}
