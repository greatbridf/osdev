#![no_std]
#![no_main]
#![feature(c_size_t)]
#![feature(concat_idents)]
#![feature(arbitrary_self_types)]
#![feature(get_mut_unchecked)]
extern crate alloc;

#[allow(warnings)]
mod bindings;

mod driver;
mod fs;
mod hash;
mod io;
mod kernel;
mod net;
mod path;
mod prelude;
mod rcu;
mod sync;

use alloc::{ffi::CString, sync::Arc};
use bindings::root::types::elf::{elf32_load, elf32_load_data};
use kernel::vfs::{
    dentry::Dentry,
    mount::{do_mount, MS_NOATIME, MS_NODEV, MS_NOSUID, MS_RDONLY},
    FsContext,
};
use path::Path;
use prelude::*;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println_fatal!("panicked at {:?}\n\t\t{}", info.location(), info.message());

    unsafe { bindings::root::freeze() };
}

extern "C" {
    fn _do_allocate(size: usize) -> *mut core::ffi::c_void;
    fn _do_deallocate(ptr: *mut core::ffi::c_void, size: core::ffi::c_size_t) -> i32;
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

#[no_mangle]
pub extern "C" fn late_init_rust(out_sp: *mut usize, out_ip: *mut usize) {
    driver::e1000e::register_e1000e_driver();
    driver::ahci::register_ahci_driver();

    fs::procfs::init();
    fs::fat32::init();

    // mount fat32 /mnt directory
    let fs_context = FsContext::get_current();
    let mnt_dir = Dentry::open(&fs_context, Path::new(b"/mnt/").unwrap(), true).unwrap();

    mnt_dir.mkdir(0o755).unwrap();

    do_mount(
        &mnt_dir,
        "/dev/sda",
        "/mnt",
        "fat32",
        MS_RDONLY | MS_NOATIME | MS_NODEV | MS_NOSUID,
    )
    .unwrap();

    let init = Dentry::open(&fs_context, Path::new(b"/mnt/busybox").unwrap(), true)
        .expect("kernel panic: init not found!");

    let argv = vec![
        CString::new("/mnt/busybox").unwrap(),
        CString::new("sh").unwrap(),
        CString::new("/mnt/initsh").unwrap(),
    ];

    let envp = vec![
        CString::new("LANG=C").unwrap(),
        CString::new("HOME=/root").unwrap(),
        CString::new("PATH=/mnt").unwrap(),
        CString::new("PWD=/").unwrap(),
    ];

    let argv_array = argv.iter().map(|x| x.as_ptr()).collect::<Vec<_>>();
    let envp_array = envp.iter().map(|x| x.as_ptr()).collect::<Vec<_>>();

    // load init
    let mut load_data = elf32_load_data {
        exec_dent: Arc::into_raw(init) as *mut _,
        argv: argv_array.as_ptr(),
        argv_count: argv_array.len(),
        envp: envp_array.as_ptr(),
        envp_count: envp_array.len(),
        ip: 0,
        sp: 0,
    };

    let result = unsafe { elf32_load(&mut load_data) };
    if result != 0 {
        println_fatal!("Failed to load init: {}", result);
    }

    unsafe {
        *out_sp = load_data.sp;
        *out_ip = load_data.ip;
    }
}
