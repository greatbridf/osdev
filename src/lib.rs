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
mod io;
mod kernel;
mod kernel_init;
mod net;
mod path;
mod prelude;
mod rcu;
mod sync;

use alloc::{ffi::CString, sync::Arc};
use core::alloc::{GlobalAlloc, Layout};
use elf::ParsedElf32;
use eonix_mm::paging::PFN;
use eonix_runtime::{run::FutureRun, scheduler::Scheduler, task::Task};
use kernel::{
    mem::Page,
    pcie::init_pcie,
    task::{KernelStack, ProcessBuilder, ProcessList, ThreadBuilder, ThreadRunnable},
    vfs::{
        dentry::Dentry,
        mount::{do_mount, MS_NOATIME, MS_NODEV, MS_NOSUID, MS_RDONLY},
        FsContext,
    },
    CharDevice,
};
use path::Path;
use prelude::*;

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
}

struct Allocator;
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
static ALLOCATOR: Allocator = Allocator;

#[no_mangle]
pub extern "C" fn kernel_init(early_kstack_pfn: PFN) -> ! {
    init_pcie().expect("Unable to initialize PCIe bus");

    // To satisfy the `Scheduler` "preempt count == 0" assertion.
    eonix_preempt::disable();

    // We need root dentry to be present in constructor of `FsContext`.
    // So call `init_vfs` first, then `init_multitasking`.
    Scheduler::init_local_scheduler::<KernelStack>();

    Scheduler::get().spawn::<KernelStack, _>(FutureRun::new(init_process(early_kstack_pfn)));

    unsafe {
        // SAFETY: `preempt::count()` == 1.
        Scheduler::goto_scheduler_noreturn()
    }
}

async fn init_process(early_kstack_pfn: PFN) {
    unsafe { Page::from_raw(early_kstack_pfn) };

    kernel::syscall::register_syscalls();
    CharDevice::init().unwrap();

    // We might want the serial initialized as soon as possible.
    driver::serial::init().unwrap();

    driver::e1000e::register_e1000e_driver();
    driver::ahci::register_ahci_driver();

    fs::tmpfs::init();
    fs::procfs::init();
    fs::fat32::init();

    kernel::smp::bootstrap_smp();

    let (ip, sp, mm_list) = {
        // mount fat32 /mnt directory
        let fs_context = FsContext::global();
        let mnt_dir = Dentry::open(fs_context, Path::new(b"/mnt/").unwrap(), true).unwrap();

        mnt_dir.mkdir(0o755).unwrap();

        do_mount(
            &mnt_dir,
            "/dev/sda",
            "/mnt",
            "fat32",
            MS_RDONLY | MS_NOATIME | MS_NODEV | MS_NOSUID,
        )
        .unwrap();

        let init = Dentry::open(fs_context, Path::new(b"/mnt/busybox").unwrap(), true)
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
        elf.load(argv, envp).unwrap()
    };

    let thread_builder = ThreadBuilder::new().name(Arc::from(*b"busybox"));

    let mut process_list = Task::block_on(ProcessList::get().write());
    let (thread, process) = ProcessBuilder::new()
        .mm_list(mm_list)
        .thread_builder(thread_builder)
        .build(&mut process_list);

    process_list.set_init_process(process);

    // TODO!!!: Remove this.
    thread.files.open_console();

    Scheduler::get().spawn::<KernelStack, _>(ThreadRunnable::new(thread, ip, sp));
}
