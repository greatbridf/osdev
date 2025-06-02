use super::cpu::init_localcpu;
use crate::{
    kernel::{cpu::local_cpu, mem::paging::Page, task::KernelStack},
    println_debug,
};
use arch::define_smp_bootstrap;
use eonix_mm::address::Addr as _;
use eonix_runtime::scheduler::Scheduler;

define_smp_bootstrap!(4, ap_entry, {
    let page = Page::alloc_order(9);
    let stack_bottom = page.range().end();
    core::mem::forget(page);

    // Physical address is used for init state APs.
    stack_bottom.addr() as u64
});

unsafe extern "C" fn ap_entry() -> ! {
    init_localcpu();

    Scheduler::init_local_scheduler::<KernelStack>();
    println_debug!("AP{} started", local_cpu().cpuid());

    eonix_preempt::disable();
    arch::enable_irqs();

    // TODO!!!!!: Free the stack after having switched to idle task.
    unsafe {
        // SAFETY: `preempt::count()` == 1.
        Scheduler::goto_scheduler_noreturn()
    }
}

pub fn bootstrap_smp() {
    eonix_preempt::disable();
    unsafe {
        // SAFETY: Preemption is disabled.
        local_cpu().bootstrap_cpus();
        wait_cpus_online();
    }
    eonix_preempt::enable();
}
