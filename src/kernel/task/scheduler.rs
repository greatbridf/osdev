use core::{
    ptr::NonNull,
    sync::atomic::{compiler_fence, fence, Ordering},
};

use crate::{kernel::console::println_trace, prelude::*, sync::preempt};

use alloc::sync::Arc;

use super::{readyqueue::rq_thiscpu, Thread};

pub struct Scheduler;

/// Idle task thread
/// All the idle task threads belongs to `pid 0` and are pinned to the current cpu.
#[arch::define_percpu]
static IDLE_TASK: Option<NonNull<Thread>> = None;

/// Current thread
#[arch::define_percpu]
static CURRENT: Option<NonNull<Thread>> = None;

impl Scheduler {
    /// `Scheduler` might be used in various places. Do not hold it for a long time.
    ///
    /// # Safety
    /// The locked returned by this function should be locked with `lock_irq` to prevent from
    /// rescheduling during access to the scheduler. Disabling preemption will do the same.
    ///
    /// Drop the lock before calling `schedule`.
    pub fn get() -> &'static Self {
        static GLOBAL_SCHEDULER: Scheduler = Scheduler;
        &GLOBAL_SCHEDULER
    }

    /// # Safety
    /// We should never "inspect" a change in `current`.
    /// The change of `CURRENT` will only happen in the scheduler. And if we are preempted,
    /// when we DO return, the `CURRENT` will be the same and remain valid.
    pub fn current<'lt>() -> BorrowedArc<'lt, Thread> {
        BorrowedArc::from_raw(CURRENT.get().unwrap().as_ptr())
    }

    /// # Safety
    /// Idle task should never change so we can borrow it without touching the refcount.
    pub fn idle_task() -> BorrowedArc<'static, Thread> {
        BorrowedArc::from_raw(IDLE_TASK.get().unwrap().as_ptr())
    }

    pub unsafe fn set_idle_and_current(thread: Arc<Thread>) {
        // We don't wake the idle thread to prevent from accidentally being scheduled there.
        thread.init(idle_task as *const () as usize);

        let old = IDLE_TASK.swap(NonNull::new(Arc::into_raw(thread.clone()) as *mut _));
        assert!(old.is_none(), "Idle task is already set");

        let old = CURRENT.swap(NonNull::new(Arc::into_raw(thread) as *mut _));
        assert!(old.is_none(), "Current is already set");
    }

    pub fn activate(&self, thread: &Arc<Thread>) {
        // TODO: Select an appropriate ready queue to enqueue.
        if !thread.on_rq.swap(true, Ordering::AcqRel) {
            rq_thiscpu().lock_irq().put(thread.clone());
        }
    }
}

impl Scheduler {
    /// Go to idle task. Call this with `preempt_count == 1`.
    /// The preempt count will be decremented by this function.
    ///
    /// # Safety
    /// We might never return from here.
    /// Drop all variables that take ownership of some resource before calling this function.
    pub fn schedule() {
        might_sleep!(1);

        // Make sure all works are done before scheduling.
        compiler_fence(Ordering::SeqCst);

        // TODO!!!!!: Use of reference here needs further consideration.
        //
        // Since we might never return to here, we can't take ownership of `current()`.
        // Is it safe to believe that `current()` will never change across calls?
        context_switch_light(&Thread::current(), &Scheduler::idle_task());
        preempt::enable();
    }

    pub fn schedule_noreturn() -> ! {
        preempt::disable();
        Self::schedule();
        panic!("Scheduler::schedule_noreturn(): Should never return")
    }
}

fn context_switch_light(from: &Arc<Thread>, to: &Arc<Thread>) {
    unsafe {
        arch::TaskContext::switch_to(
            &mut *from.get_context_mut_ptr(),
            &mut *to.get_context_mut_ptr(),
        );
    }
}

/// In this function, we should see `preempt_count == 1`.
extern "C" fn idle_task() {
    loop {
        debug_assert_eq!(preempt::count(), 1);

        let next = rq_thiscpu().lock().get();
        match next {
            None => {
                if Thread::current().state.is_runnable() {
                    println_trace!(
                        "trace_scheduler",
                        "Returning to tid({}) without doing context switch",
                        Thread::current().tid
                    );

                    // Previous thread is `Running`, Return to current running thread
                    // without changing its state.
                    context_switch_light(&Scheduler::idle_task(), &Thread::current());
                } else {
                    // Halt the cpu and rerun the loop.
                    arch::halt();
                }
                continue;
            }
            Some(next) => {
                println_trace!(
                    "trace_scheduler",
                    "Switching from tid({}) to tid({})",
                    Thread::current().tid,
                    next.tid
                );

                debug_assert_ne!(
                    next.tid,
                    Thread::current().tid,
                    "Switching to the same thread"
                );

                next.process.mm_list.switch_page_table();

                if let Some(thread_pointer) =
                    CURRENT.swap(NonNull::new(Arc::into_raw(next) as *mut _))
                {
                    let thread = unsafe { Arc::from_raw(thread_pointer.as_ptr()) };
                    let mut rq = rq_thiscpu().lock();

                    if thread.state.is_runnable() {
                        rq.put(thread);
                    } else {
                        thread.on_rq.store(false, Ordering::Release);
                    }
                }
            }
        }

        unsafe {
            // SAFETY: We are in the idle task where preemption is disabled.
            //         So we can safely load the thread area and interrupt stack.
            Thread::current().load_interrupt_stack();
            Thread::current().load_thread_area32();
        }

        // TODO!!!: If the task comes from another cpu, we need to sync.
        //
        // The other cpu should see the changes of kernel stack of the target thread
        // made in this cpu.
        //
        // Can we find a better way other than `fence`s?
        fence(Ordering::SeqCst);
        context_switch_light(&Scheduler::idle_task(), &Thread::current());
        fence(Ordering::SeqCst);
    }
}
