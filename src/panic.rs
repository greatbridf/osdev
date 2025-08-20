use core::ffi::c_void;

use eonix_log::println_fatal;
use unwinding::abi::{
    UnwindContext, UnwindReasonCode, _Unwind_Backtrace, _Unwind_GetIP, _Unwind_GetRegionStart,
};

pub fn stack_trace() {
    struct CallbackData {
        counter: usize,
    }

    extern "C" fn callback(unwind_ctx: &UnwindContext<'_>, arg: *mut c_void) -> UnwindReasonCode {
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

    println_fatal!("<<<<<<<<<< 8< CUT HERE 8< <<<<<<<<<<");

    let mut data = CallbackData { counter: 0 };
    _Unwind_Backtrace(callback, &raw mut data as *mut c_void);
}
