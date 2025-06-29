#![no_std]
#![no_main]
#![feature(c_size_t)]
#![feature(concat_idents)]
#![feature(arbitrary_self_types)]
#![feature(get_mut_unchecked)]
#![feature(macro_metavar_expr)]

extern crate alloc;

mod driver;
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

use crate::kernel::task::alloc_pid;
use alloc::{ffi::CString, sync::Arc};
use core::{
    hint::spin_loop,
    sync::atomic::{AtomicBool, Ordering},
};
use eonix_hal::{processor::CPU, traits::trap::IrqState, trap::disable_irqs_save};
use eonix_mm::address::PRange;
use eonix_runtime::{run::FutureRun, scheduler::Scheduler, task::Task};
use kernel::{
    mem::GlobalPageAlloc,
    task::{
        new_thread_runnable, KernelStack, ProcessBuilder, ProcessList, ProgramLoader, ThreadBuilder,
    },
    vfs::{
        dentry::Dentry,
        mount::{do_mount, MS_NOATIME, MS_NODEV, MS_NOSUID, MS_RDONLY},
        FsContext,
    },
    CharDevice,
};
use kernel_init::setup_memory;
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

    loop {}
}

static BSP_OK: AtomicBool = AtomicBool::new(false);

#[eonix_hal::main]
fn kernel_init(mut data: eonix_hal::bootstrap::BootStrapData) -> ! {
    setup_memory(&mut data);

    #[cfg(target_arch = "riscv64")]
    {
        driver::sbi_console::init_console();
    }

    kernel::pcie::init_pcie().expect("Unable to initialize PCIe bus");

    // To satisfy the `Scheduler` "preempt count == 0" assertion.
    eonix_preempt::disable();

    // We need root dentry to be present in constructor of `FsContext`.
    // So call `init_vfs` first, then `init_multitasking`.
    Scheduler::init_local_scheduler::<KernelStack>();

    Scheduler::get().spawn::<KernelStack, _>(FutureRun::new(init_process(data.get_early_stack())));

    BSP_OK.store(true, Ordering::Release);

    drop(data);
    unsafe {
        // SAFETY: `preempt::count()` == 1.
        Scheduler::goto_scheduler_noreturn()
    }
}

#[eonix_hal::ap_main]
fn kernel_ap_main(_stack_range: PRange) -> ! {
    while BSP_OK.load(Ordering::Acquire) == false {
        // Wait for BSP to finish initializing.
        spin_loop();
    }

    Scheduler::init_local_scheduler::<KernelStack>();
    println_debug!("AP{} started", CPU::local().cpuid());

    eonix_preempt::disable();

    // TODO!!!!!: Free the stack after having switched to idle task.
    unsafe {
        // SAFETY: `preempt::count()` == 1.
        Scheduler::goto_scheduler_noreturn()
    }
}

async fn init_process(early_kstack: PRange) {
    unsafe {
        let irq_ctx = disable_irqs_save();

        // SAFETY: IRQ is disabled.
        GlobalPageAlloc::add_pages(early_kstack);

        irq_ctx.restore();
    }

    CharDevice::init().unwrap();

    #[cfg(target_arch = "x86_64")]
    {
        // We might want the serial initialized as soon as possible.
        driver::serial::init().unwrap();
        driver::e1000e::register_e1000e_driver();
        driver::ahci::register_ahci_driver();
    }

    #[cfg(target_arch = "riscv64")]
    {
        driver::serial::init().unwrap();
        driver::virtio::init_virtio_devices();
        driver::e1000e::register_e1000e_driver();
        driver::ahci::register_ahci_driver();
        driver::goldfish_rtc::probe();
    }

    fs::tmpfs::init();
    fs::procfs::init();
    fs::fat32::init();
    fs::ext4::init();

    let load_info = {
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

        let init_names = [&b"/init"[..], &b"/sbin/init"[..], &b"/mnt/initsh"[..]];

        let mut init_name = None;
        let mut init = None;
        for name in init_names {
            if let Ok(dentry) = Dentry::open(fs_context, Path::new(name).unwrap(), true) {
                if dentry.is_valid() {
                    init_name = Some(CString::new(name).unwrap());
                    init = Some(dentry);
                    break;
                }
            }
        }

        let init = init.expect("No init binary found in the system.");
        let init_name = init_name.unwrap();

        let argv = vec![init_name.clone()];

        let envp = vec![
            CString::new("LANG=C").unwrap(),
            CString::new("HOME=/root").unwrap(),
            CString::new("PATH=/mnt").unwrap(),
            CString::new("PWD=/").unwrap(),
        ];

        ProgramLoader::parse(fs_context, init_name, init.clone(), argv, envp)
            .expect("Failed to parse init program")
            .load()
            .expect("Failed to load init program")
    };

    let thread_builder = ThreadBuilder::new()
        .name(Arc::from(&b"busybox"[..]))
        .entry(load_info.entry_ip, load_info.sp);

    let mut process_list = Task::block_on(ProcessList::get().write());
    let (thread, process) = ProcessBuilder::new()
        .pid(alloc_pid())
        .mm_list(load_info.mm_list)
        .thread_builder(thread_builder)
        .build(&mut process_list);

    process_list.set_init_process(process);

    // TODO!!!: Remove this.
    thread.files.open_console();

    Scheduler::get().spawn::<KernelStack, _>(new_thread_runnable(thread));
}
