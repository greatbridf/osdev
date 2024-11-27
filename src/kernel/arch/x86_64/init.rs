use super::{interrupt::setup_idt, GDT_OBJECT, TSS_OBJECT};
use crate::{
    kernel::{
        arch::interrupt::APIC_BASE,
        mem::{paging::Page, phys::PhysPtr as _},
        smp,
    },
    println_debug, println_info,
    sync::preempt,
};
use arch::{
    task::pause,
    x86_64::{gdt::GDT, task::TSS},
};
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

unsafe fn init_gdt_tss_thiscpu() {
    preempt::disable();
    let gdt_ref = unsafe { GDT_OBJECT.as_mut() };
    let tss_ref = unsafe { TSS_OBJECT.as_mut() };
    *gdt_ref = Some(GDT::new());
    *tss_ref = Some(TSS::new());

    if let Some(gdt) = gdt_ref.as_mut() {
        if let Some(tss) = tss_ref.as_mut() {
            gdt.set_tss(tss as *mut _ as u64);
        } else {
            panic!("TSS is not initialized");
        }

        unsafe { gdt.load() };
    } else {
        panic!("GDT is not initialized");
    }

    preempt::enable();
}

/// Initialization routine for all CPUs.
pub unsafe fn init_cpu() {
    arch::x86_64::io::enable_sse();

    let area = smp::alloc_percpu_area();
    smp::set_percpu_area(area);
    init_gdt_tss_thiscpu();

    setup_idt();

    APIC_BASE.spurious().write(0x1ff);
    APIC_BASE.task_priority().write(0);
    APIC_BASE.timer_divide().write(0x3); // Divide by 16
    APIC_BASE.timer_register().write(0x20040);

    // TODO: Get the bus frequency from...?
    let freq = 800;
    let count = freq * 1_000_000 / 16 / 100;
    APIC_BASE.timer_initial_count().write(count as u32);

    let cpu = CPU_COUNT.fetch_add(1, Ordering::Relaxed);
    if cpu != 0 {
        // Application processor
        println_debug!("AP{} started", cpu);
    }
}

#[no_mangle]
pub static BOOT_SEMAPHORE: AtomicU32 = AtomicU32::new(0);
#[no_mangle]
pub static BOOT_STACK: AtomicUsize = AtomicUsize::new(0);

pub static CPU_COUNT: AtomicUsize = AtomicUsize::new(0);

#[no_mangle]
pub unsafe extern "C" fn ap_entry(_stack_start: u64) {
    init_cpu();

    // TODO!!!!!: Set up idle task.

    // TODO!!!!!: Free the stack before switching to idle task.

    loop {}
}

pub unsafe fn bootstrap_cpus() {
    let icr = APIC_BASE.interrupt_command();

    icr.write(0xc4500);
    while icr.read() & 0x1000 != 0 {
        pause();
    }

    icr.write(0xc4601);
    while icr.read() & 0x1000 != 0 {
        pause();
    }

    while CPU_COUNT.load(Ordering::Acquire) != 3 {
        if BOOT_STACK.load(Ordering::Acquire) == 0 {
            let page = Page::alloc_many(9);
            let stack_start = page.as_cached().as_ptr::<()>() as usize;
            core::mem::forget(page);

            BOOT_STACK.store(stack_start, Ordering::Release);
        }
        pause();
    }

    println_info!("Processors startup finished");
}
