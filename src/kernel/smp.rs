use super::cpu::init_thiscpu;
use crate::{
    kernel::{
        cpu::current_cpu,
        mem::{paging::Page, phys::PhysPtr as _},
        task::KernelStack,
    },
    println_debug,
};
use arch::define_smp_bootstrap;
use eonix_runtime::scheduler::Scheduler;

define_smp_bootstrap!(4, ap_entry, {
    let page = Page::alloc_many(9);
    let stack_bottom = page.as_cached().as_ptr::<()>() as usize + page.len();
    core::mem::forget(page);
    stack_bottom
});

unsafe extern "C" fn ap_entry() -> ! {
    init_thiscpu();
    Scheduler::init_local_scheduler::<KernelStack>();
    println_debug!("AP{} started", current_cpu().cpuid());

    eonix_preempt::disable();
    arch::enable_irqs();

    // TODO!!!!!: Free the stack after having switched to idle task.
    unsafe {
        // SAFETY: `preempt::count()` == 1.
        Scheduler::goto_scheduler_noreturn()
    }
}

pub unsafe fn bootstrap_smp() {
    current_cpu().bootstrap_cpus();
    wait_cpus_online();
}
