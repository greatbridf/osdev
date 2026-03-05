use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;

use eonix_hal::{extern_symbol_addr, extern_symbol_value};

pub fn run_late_init() {
    let count = extern_symbol_value!(LATE_INIT_FUNCTION_COUNT);
    let addr = extern_symbol_addr!(LATE_INIT_FUNCTIONS, fn());

    let functions = unsafe { core::slice::from_raw_parts(addr, count) };

    for func in functions {
        func();
    }
}

pub async fn run_late_init_async() {
    let count = extern_symbol_value!(LATE_INIT_ASYNC_FUNCTION_COUNT);
    let addr = extern_symbol_addr!(
        LATE_INIT_ASYNC_FUNCTIONS,
        fn() -> Pin<Box<dyn Future<Output = ()> + Send>>
    );

    let functions = unsafe { core::slice::from_raw_parts(addr, count) };

    for func in functions {
        func().await;
    }
}
