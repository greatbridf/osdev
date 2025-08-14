#![no_std]
#![no_main]
#![feature(allocator_api)]
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
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use eonix_hal::{
    arch_exported::bootstrap::shutdown,
    context::TaskContext,
    processor::{halt, CPU, CPU_COUNT},
    traits::{context::RawTaskContext, trap::IrqState},
    trap::disable_irqs_save,
};
use eonix_mm::address::PRange;
use eonix_runtime::{executor::Stack, scheduler::RUNTIME};
use kernel::{
    mem::GlobalPageAlloc,
    task::{KernelStack, ProcessBuilder, ProcessList, ProgramLoader, ThreadBuilder},
    vfs::{
        dentry::Dentry,
        inode::Mode,
        mount::{do_mount, MS_NOATIME, MS_NODEV, MS_NOSUID, MS_RDONLY},
        FsContext,
    },
    CharDevice,
};
use kernel_init::setup_memory;
use path::Path;
use prelude::*;

#[cfg(any(target_arch = "riscv64", target_arch = "loongarch64"))]
fn do_panic() -> ! {
    shutdown();
}

#[cfg(not(any(target_arch = "riscv64", target_arch = "loongarch64")))]
fn do_panic() -> ! {
    // Spin forever.
    loop {
        spin_loop();
    }
}

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

    do_panic()
}

static BSP_OK: AtomicBool = AtomicBool::new(false);
static CPU_SHUTTING_DOWN: AtomicUsize = AtomicUsize::new(0);

fn shutdown_system() -> ! {
    let cpu_count = CPU_COUNT.load(Ordering::Relaxed);

    if CPU_SHUTTING_DOWN.fetch_add(1, Ordering::AcqRel) + 1 == cpu_count {
        println_info!("All CPUs are shutting down. Gracefully powering off...");
        shutdown();
    } else {
        println_info!(
            "CPU {} is shutting down. Waiting for other CPUs...",
            CPU::local().cpuid()
        );

        loop {
            halt();
        }
    }
}

#[eonix_hal::main]
fn kernel_init(mut data: eonix_hal::bootstrap::BootStrapData) -> ! {
    setup_memory(&mut data);

    #[cfg(target_arch = "riscv64")]
    {
        driver::sbi_console::init_console();
    }

    BSP_OK.store(true, Ordering::Release);

    RUNTIME.spawn(init_process(data.get_early_stack()));

    drop(data);

    let mut ctx = TaskContext::new();
    let stack_bottom = {
        let stack = KernelStack::new();
        let bottom = stack.get_bottom().addr().get();
        core::mem::forget(stack);

        bottom
    };
    ctx.set_interrupt_enabled(true);
    ctx.set_program_counter(standard_main as usize);
    ctx.set_stack_pointer(stack_bottom);

    unsafe {
        TaskContext::switch_to_noreturn(&mut ctx);
    }
}

#[eonix_hal::ap_main]
fn kernel_ap_main(_stack_range: PRange) -> ! {
    while BSP_OK.load(Ordering::Acquire) == false {
        // Wait for BSP to finish initializing.
        spin_loop();
    }

    println_debug!("AP{} started", CPU::local().cpuid());

    let mut ctx = TaskContext::new();
    let stack_bottom = {
        let stack = KernelStack::new();
        let bottom = stack.get_bottom().addr().get();
        core::mem::forget(stack);

        bottom
    };
    ctx.set_interrupt_enabled(true);
    ctx.set_program_counter(standard_main as usize);
    ctx.set_stack_pointer(stack_bottom);

    unsafe {
        TaskContext::switch_to_noreturn(&mut ctx);
    }
}

fn standard_main() -> ! {
    RUNTIME.enter();
    shutdown_system();
}

async fn init_process(early_kstack: PRange) {
    unsafe {
        let irq_ctx = disable_irqs_save();

        // SAFETY: IRQ is disabled.
        GlobalPageAlloc::add_pages(early_kstack);

        irq_ctx.restore();
    }

    kernel::pcie::init_pcie().expect("Unable to initialize PCIe bus");

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

    #[cfg(target_arch = "loongarch64")]
    {
        driver::serial::init().unwrap();
        driver::virtio::init_virtio_devices();
        driver::e1000e::register_e1000e_driver();
        driver::ahci::register_ahci_driver();
    }

    fs::tmpfs::init();
    fs::procfs::init();
    fs::fat32::init();
    fs::ext4::init();

    let load_info = {
        // mount fat32 /mnt directory
        let fs_context = FsContext::global();
        let mnt_dir = Dentry::open(fs_context, Path::new(b"/mnt/").unwrap(), true).unwrap();

        mnt_dir.mkdir(Mode::new(0o755)).unwrap();

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
            .await
            .expect("Failed to load init program")
    };

    let thread_builder = ThreadBuilder::new()
        .name(Arc::from(&b"busybox"[..]))
        .entry(load_info.entry_ip, load_info.sp);

    let mut process_list = ProcessList::get().write().await;
    let (thread, process) = ProcessBuilder::new()
        .pid(alloc_pid())
        .mm_list(load_info.mm_list)
        .thread_builder(thread_builder)
        .build(&mut process_list);

    process_list.set_init_process(process);

    // TODO!!!: Remove this.
    thread.files.open_console();

    RUNTIME.spawn(thread.run());
}
