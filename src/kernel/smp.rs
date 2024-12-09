use alloc::{format, sync::Arc};
use arch::define_smp_bootstrap;

use crate::{
    kernel::{
        cpu::current_cpu,
        mem::{paging::Page, phys::PhysPtr as _},
        task::{Process, Thread},
    },
    println_debug,
    sync::preempt,
};

use super::{
    cpu::init_thiscpu,
    task::{ProcessList, Scheduler},
};

define_smp_bootstrap!(4, ap_entry, {
    let page = Page::alloc_many(9);
    let stack_bottom = page.as_cached().as_ptr::<()>() as usize + page.len();
    core::mem::forget(page);
    stack_bottom
});

unsafe extern "C" fn ap_entry() {
    init_thiscpu();
    println_debug!("AP{} started", current_cpu().cpuid());

    {
        let mut procs = ProcessList::get().lock_nosleep();
        let idle_process = procs.idle_process().clone();

        let idle_thread_name = format!("[kernel idle#AP{}]", 0);
        let idle_thread = Thread::new_for_init(
            Arc::from(idle_thread_name.as_bytes()),
            Process::alloc_pid(),
            &idle_process,
            procs.as_mut(),
        );
        Scheduler::set_idle_and_current(idle_thread);
    }

    preempt::disable();
    arch::enable_irqs();

    // TODO!!!!!: Free the stack after having switched to idle task.

    // TODO: Temporary solution: we will never access this later on.
    let mut unuse_ctx = arch::TaskContext::new();
    let mut unused_area = [0u8; 64];
    unuse_ctx.init(0, unused_area.as_mut_ptr() as usize);
    unsafe {
        arch::TaskContext::switch_to(
            &mut unuse_ctx, // We will never come back
            &mut *Scheduler::idle_task().get_context_mut_ptr(),
        );
    }
    arch::freeze()
}

pub unsafe fn bootstrap_smp() {
    current_cpu().bootstrap_cpus();
    wait_cpus_online();
}
