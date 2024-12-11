#![no_std]
#![no_main]
#![feature(c_size_t)]
#![feature(concat_idents)]
#![feature(arbitrary_self_types)]
#![feature(get_mut_unchecked)]
#![feature(macro_metavar_expr)]
#![feature(naked_functions)]

extern crate alloc;

#[allow(warnings)]
mod bindings;

mod driver;
mod elf;
mod fs;
mod hash;
mod intrusive_list;
mod io;
mod kernel;
mod net;
mod path;
mod prelude;
mod rcu;
mod sync;

use alloc::ffi::CString;
use core::{
    alloc::{GlobalAlloc, Layout},
    arch::{asm, global_asm},
};
use elf::ParsedElf32;
use kernel::{
    cpu::init_thiscpu,
    task::{init_multitasking, Scheduler, Thread},
    vfs::{
        dentry::Dentry,
        mount::{do_mount, MS_NOATIME, MS_NODEV, MS_NOSUID, MS_RDONLY},
        FsContext,
    },
    CharDevice,
};
use path::Path;
use prelude::*;
use sync::preempt;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
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

    arch::freeze()
}

extern "C" {
    fn _do_allocate(size: usize) -> *mut core::ffi::c_void;
    fn _do_deallocate(ptr: *mut core::ffi::c_void, size: core::ffi::c_size_t) -> i32;
    fn init_pci();
}

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

global_asm!(
    r"
    .globl to_init_process
    to_init_process:
        push %rbp
        mov %rbx, %rdi
        jmp {}
    ",
    sym init_process,
    options(att_syntax)
);

extern "C" {
    fn to_init_process();
}

#[no_mangle]
pub extern "C" fn rust_kinit(early_kstack_pfn: usize) -> ! {
    // We don't call global constructors.
    // Rust doesn't need that, and we're not going to use global variables in C++.
    unsafe { init_thiscpu() };

    kernel::interrupt::init().unwrap();

    // TODO: Move this to rust.
    unsafe { init_pci() };

    kernel::vfs::mount::init_vfs().unwrap();

    // To satisfy the `Scheduler` "preempt count == 0" assertion.
    preempt::disable();

    // We need root dentry to be present in constructor of `FsContext`.
    // So call `init_vfs` first, then `init_multitasking`.
    unsafe { init_multitasking(init_process) };

    let mut unuse_ctx = arch::TaskContext::new();
    // TODO: Temporary solution: we will never access this later on.
    unuse_ctx.init(
        to_init_process as usize,
        early_kstack_pfn + 0x1000 + 0xffffff0000000000,
    );
    unsafe {
        arch::TaskContext::switch_to(
            &mut unuse_ctx, // We will never come back
            &mut *Scheduler::idle_task().get_context_mut_ptr(),
        );
    }

    arch::freeze()
}

/// We enter this function with `preempt count == 0`
extern "C" fn init_process(/* early_kstack_pfn: usize */) {
    // TODO!!! Should free pass eraly_kstack_pfn and free !!!
    // unsafe { Page::take_pfn(early_kstack_pfn, 9) };
    preempt::enable();

    kernel::syscall::register_syscalls();
    CharDevice::init().unwrap();

    // We might want the serial initialized as soon as possible.
    driver::serial::init().unwrap();

    driver::e1000e::register_e1000e_driver();
    driver::ahci::register_ahci_driver();

    fs::procfs::init();
    fs::fat32::init();

    unsafe { kernel::smp::bootstrap_smp() };

    let (ip, sp) = {
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
            .expect("busybox should be present in /mnt");

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

        let elf = ParsedElf32::parse(init.clone()).unwrap();
        elf.load(&Thread::current().process.mm_list, argv, envp)
            .unwrap()
    };

    Thread::current().files.open_console();

    unsafe {
        asm!(
            "swapgs",
            "mov ${ds}, %rax",
            "mov %ax, %ds",
            "mov %ax, %es",
            "push ${ds}",
            "push {sp}",
            "push $0x200",
            "push ${cs}",
            "push {ip}",
            "iretq",
            ds = const 0x33,
            cs = const 0x2b,
            in("rax") 0,
            ip = in(reg) ip.0,
            sp = in(reg) sp.0,
            options(att_syntax, noreturn),
        );
    }
}
