use core::sync::atomic::{AtomicUsize, Ordering};

use eonix_hal::processor::{halt, CPU, CPU_COUNT};
use eonix_log::println_info;

static CPU_SHUTTING_DOWN: AtomicUsize = AtomicUsize::new(0);

fn shutdown() {
    #[cfg(arch_has_shutdown)]
    {
        eonix_hal::arch_exported::bootstrap::shutdown();
    }
    #[cfg(not(arch_has_shutdown))]
    {
        println_info!("Shutdown not supported on this architecture...");
    }
}

pub fn shutdown_system_nowait() {
    shutdown();

    loop {
        halt();
    }
}

pub fn shutdown_system() -> ! {
    // TODO: We don't have IPI system for now.
    shutdown_system_nowait();

    let cpu_count = CPU_COUNT.load(Ordering::Relaxed);

    if CPU_SHUTTING_DOWN.fetch_add(1, Ordering::AcqRel) + 1 == cpu_count {
        println_info!("All CPUs are shutting down. Gracefully powering off...");
        shutdown();
    } else {
        println_info!(
            "CPU {} is shutting down. Waiting for other CPUs...",
            CPU::local().cpuid()
        );
    }

    loop {
        halt();
    }
}
