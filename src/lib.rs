#![no_std]
#![no_main]
#![feature(c_size_t)]
#![feature(concat_idents)]
extern crate alloc;

#[allow(warnings)]
mod bindings;

mod io;
mod kernel;

macro_rules! dont_check {
    ($arg:expr) => {
        match $arg {
            Ok(_) => (),
            Err(_) => (),
        }
    };
}

pub(crate) use dont_check;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    dont_check!(println!("[kernel] panic: {:?}", info));

    unsafe { bindings::root::freeze() };
}

extern "C" {
    fn _do_allocate(size: usize) -> *mut core::ffi::c_void;
    fn _do_deallocate(
        ptr: *mut core::ffi::c_void,
        size: core::ffi::c_size_t,
    ) -> i32;
}

use core::alloc::{GlobalAlloc, Layout};

struct Allocator {}
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let result = _do_allocate(layout.size());

        if result.is_null() {
            core::ptr::null_mut()
        } else {
            result as *mut u8
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        match _do_deallocate(ptr as *mut core::ffi::c_void, layout.size()) {
            0 => (),
            _ => panic!("Failed to deallocate memory"),
        }
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator {};

#[repr(C)]
#[allow(dead_code)]
struct Fp {
    fp: *const core::ffi::c_void,
}

unsafe impl Sync for Fp {}

#[allow(unused_macros)]
macro_rules! late_init {
    ($name:ident, $func:ident) => {
        #[used]
        #[link_section = ".late_init"]
        static $name: $crate::Fp = $crate::Fp {
            fp: $func as *const core::ffi::c_void,
        };
    };
}

#[allow(unused_imports)]
pub(crate) use late_init;
