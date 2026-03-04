use eonix_hal::{extern_symbol_addr, extern_symbol_value};

pub fn run_late_init() {
    let count = extern_symbol_value!(LATE_INIT_FUNCTION_COUNT);
    let addr = extern_symbol_addr!(LATE_INIT_FUNCTIONS, fn());

    let functions = unsafe { core::slice::from_raw_parts(addr, count) };

    for func in functions {
        func();
    }
}
