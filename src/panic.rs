use core::panic::PanicInfo;

use eonix_log::println_fatal;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(location) = info.location() {
        println_fatal!(
            "panicked at {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
    } else {
        println_fatal!("panicked at <UNKNOWN>");
    }
    println_fatal!();
    println_fatal!("{}", info.message());

    #[cfg(arch_has_stacktrace)]
    stacktrace::print_stacktrace();

    panic_forever();
}

fn panic_forever() -> ! {
    #[cfg(arch_has_shutdown)]
    {
        eonix_hal::arch_exported::bootstrap::shutdown();
    }
    #[cfg(not(arch_has_shutdown))]
    {
        // Spin forever
        loop {
            core::hint::spin_loop();
        }
    }
}

#[cfg(arch_has_stacktrace)]
mod stacktrace {
    extern crate unwinding;

    use core::ffi::c_void;

    use eonix_log::println_fatal;
    use unwinding::abi::{
        UnwindContext, UnwindReasonCode, _Unwind_Backtrace, _Unwind_GetIP,
        _Unwind_GetRegionStart,
    };

    struct CallbackData {
        counter: usize,
    }

    extern "C" fn unwind_print(
        unwind_ctx: &UnwindContext<'_>, arg: *mut c_void,
    ) -> UnwindReasonCode {
        let data = unsafe { &mut *(arg as *mut CallbackData) };
        data.counter += 1;

        println_fatal!(
            "{:4}: {:#018x} - <unknown> at function {:#018x}",
            data.counter,
            _Unwind_GetIP(unwind_ctx),
            _Unwind_GetRegionStart(unwind_ctx),
        );

        UnwindReasonCode::NO_REASON
    }

    pub fn print_stacktrace() {
        println_fatal!("--------------8< CUT HERE 8<--------------");
        println_fatal!("Stacktrace:");
        println_fatal!();

        let mut data = CallbackData { counter: 0 };
        _Unwind_Backtrace(unwind_print, &raw mut data as *mut c_void);

        println_fatal!("--------------8< CUT HERE 8<--------------");
    }
}
