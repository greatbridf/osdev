use super::{GDT_OBJECT, TSS_OBJECT};
use crate::{
    kernel::{
        mem::{
            paging::Page,
            phys::{CachedPP, PhysPtr as _},
        },
        smp,
    },
    println_debug, println_info,
    sync::preempt,
};
use arch::{
    task::{pause, rdmsr},
    x86_64::{gdt::GDT, task::TSS},
};
use core::{
    arch::asm,
    sync::atomic::{AtomicU32, AtomicUsize, Ordering},
};

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

pub unsafe fn init_cpu() {
    arch::x86_64::io::enable_sse();

    let area = smp::alloc_percpu_area();
    smp::set_percpu_area(area);
    init_gdt_tss_thiscpu();
}

#[no_mangle]
pub static BOOT_SEMAPHORE: AtomicU32 = AtomicU32::new(0);
#[no_mangle]
pub static BOOT_STACK: AtomicUsize = AtomicUsize::new(0);

pub static AP_COUNT: AtomicUsize = AtomicUsize::new(0);

#[no_mangle]
pub unsafe extern "C" fn ap_entry(_stack_start: u64) {
    init_cpu();
    let cpuid = AP_COUNT.fetch_add(1, Ordering::Release);
    println_debug!("AP{} started", cpuid);

    // TODO!!!!!: Set up LAPIC and timer.

    // TODO!!!!!: Set up idle task.

    // TODO!!!!!: Free the stack before switching to idle task.

    loop {}
}

pub unsafe fn bootstrap_cpus() {
    let apic_base = rdmsr(0x1b);
    assert_eq!(apic_base & 0x800, 0x800, "LAPIC not enabled");
    assert_eq!(apic_base & 0x100, 0x100, "Is not bootstrap processor");

    let apic_base = apic_base & !0xfff;
    println_debug!("IA32_APIC_BASE: {apic_base:#x}");

    let apic_base = CachedPP::new(apic_base as usize);
    let spurious = apic_base.offset(0xf0).as_ptr::<u32>();
    let icr = apic_base.offset(0x300).as_ptr::<u32>();

    println_debug!("SPURIOUS: {:#x}", unsafe { spurious.read() });

    unsafe { icr.write_volatile(0xc4500) };

    while unsafe { icr.read_volatile() } & 0x1000 != 0 {
        pause();
    }

    unsafe { icr.write_volatile(0xc4601) };

    while unsafe { icr.read_volatile() } & 0x1000 != 0 {
        pause();
    }

    while AP_COUNT.load(Ordering::Acquire) != 3 {
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
