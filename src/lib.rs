#![no_std]
#![no_main]
#![feature(allocator_api)]
#![feature(c_size_t)]
#![feature(coerce_unsized)]
#![feature(arbitrary_self_types)]
#![feature(get_mut_unchecked)]
#![feature(macro_metavar_expr)]
#![feature(unsize)]

extern crate alloc;

#[macro_use]
extern crate static_assertions;

mod driver;
mod fs;
mod hash;
mod io;
mod kernel;
mod kernel_init;
mod net;
mod panic;
mod path;
mod prelude;
mod rcu;
mod sync;

use alloc::ffi::CString;
use core::hint::spin_loop;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use eonix_hal::arch_exported::bootstrap::shutdown;
use eonix_hal::context::TaskContext;
use eonix_hal::processor::{halt, CPU, CPU_COUNT};
use eonix_hal::symbol_addr;
use eonix_hal::traits::context::RawTaskContext;
use eonix_hal::traits::trap::IrqState;
use eonix_hal::trap::disable_irqs_save;
use eonix_mm::address::PRange;
use eonix_runtime::executor::Stack;
use eonix_runtime::scheduler::RUNTIME;
use kernel::mem::GlobalPageAlloc;
use kernel::task::{KernelStack, ProcessList, ProgramLoader};
use kernel::vfs::dentry::Dentry;
use kernel::vfs::mount::{
    do_mount, MS_NOATIME, MS_NODEV, MS_NOSUID, MS_RDONLY,
};
use kernel::vfs::types::Permission;
use kernel::vfs::FsContext;
use kernel::CharDevice;
use kernel_init::setup_memory;
use path::Path;
use prelude::*;

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
    ctx.set_program_counter(symbol_addr!(standard_main));
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
    ctx.set_program_counter(symbol_addr!(standard_main));
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
        driver::e1000e::register_e1000e_driver().await;
        driver::ahci::register_ahci_driver().await;
    }

    #[cfg(target_arch = "riscv64")]
    {
        driver::serial::init().unwrap();
        driver::virtio::init_virtio_devices();
        driver::e1000e::register_e1000e_driver().await;
        driver::ahci::register_ahci_driver().await;
        driver::goldfish_rtc::probe();
    }

    #[cfg(target_arch = "loongarch64")]
    {
        driver::serial::init().unwrap();
        driver::virtio::init_virtio_devices();
        driver::e1000e::register_e1000e_driver().await;
        driver::ahci::register_ahci_driver().await;
    }

    fs::tmpfs::init();
    fs::procfs::init().await;
    fs::fat32::init();
    // fs::ext4::init();

    let load_info = {
        // mount fat32 /mnt directory
        let fs_context = FsContext::global();
        let mnt_dir =
            Dentry::open(fs_context, Path::new(b"/mnt/").unwrap(), true)
                .await
                .unwrap();

        mnt_dir
            .mkdir(Permission::new(0o755))
            .await
            .expect("Failed to create /mnt directory");

        do_mount(
            &mnt_dir,
            "/dev/sda",
            "/mnt",
            "fat32",
            MS_RDONLY | MS_NOATIME | MS_NODEV | MS_NOSUID,
        )
        .await
        .unwrap();

        let init_names =
            [&b"/init"[..], &b"/sbin/init"[..], &b"/mnt/initsh"[..]];

        let mut init_name = None;
        let mut init = None;
        for name in init_names {
            if let Ok(dentry) =
                Dentry::open(fs_context, Path::new(name).unwrap(), true).await
            {
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
            .await
            .expect("Failed to parse init program")
            .load()
            .await
            .expect("Failed to load init program")
    };

    ProcessList::sys_init(load_info).await;
}
