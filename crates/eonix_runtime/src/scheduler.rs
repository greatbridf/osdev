use crate::{
    context::ExecutionContext,
    executor::{ExecuteStatus, OutputHandle, Stack},
    ready_queue::{cpu_rq, local_rq},
    run::{Contexted, Run},
    task::{Task, TaskAdapter, TaskHandle},
};
use alloc::sync::Arc;
use core::{
    mem::forget,
    ptr::NonNull,
    sync::atomic::{compiler_fence, Ordering},
    task::Waker,
};
use eonix_log::println_trace;
use eonix_preempt::assert_preempt_count_eq;
use eonix_spin_irq::SpinIrq as _;
use eonix_sync::{LazyLock, Spin};
use intrusive_collections::RBTree;
use pointers::BorrowedArc;

#[arch::define_percpu]
static CURRENT_TASK: Option<NonNull<Task>> = None;

#[arch::define_percpu]
static LOCAL_SCHEDULER_CONTEXT: ExecutionContext = ExecutionContext::new();

static TASKS: LazyLock<Spin<RBTree<TaskAdapter>>> =
    LazyLock::new(|| Spin::new(RBTree::new(TaskAdapter::new())));

pub struct Scheduler;

pub struct JoinHandle<Output>(Arc<Spin<OutputHandle<Output>>>)
where
    Output: Send;

impl Task {
    pub fn current<'a>() -> BorrowedArc<'a, Task> {
        unsafe {
            // SAFETY:
            // We should never "inspect" a change in `current`.
            // The change of `CURRENT` will only happen in the scheduler. And if we are preempted,
            // when we DO return, the `CURRENT` will be the same and remain valid.
            BorrowedArc::from_raw(CURRENT_TASK.get().expect("Current task should be present"))
        }
    }
}

impl<O> JoinHandle<O>
where
    O: Send,
{
    pub fn join(self) -> O {
        let Self(output) = self;
        let mut waker = Some(Waker::from(Task::current().clone()));

        loop {
            let mut locked = output.lock();
            match locked.try_resolve() {
                Some(output) => break output,
                None => {
                    if let Some(waker) = waker.take() {
                        locked.register_waiter(waker);
                    }
                }
            }
        }
    }
}

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

    pub fn init_local_scheduler<S>()
    where
        S: Stack,
    {
        let stack = S::new();

        unsafe {
            eonix_preempt::disable();
            // SAFETY: Preemption is disabled.
            let context: &mut ExecutionContext = LOCAL_SCHEDULER_CONTEXT.as_mut();
            context.set_ip(local_scheduler as _);
            context.set_sp(stack.get_bottom() as *const _ as usize);
            eonix_preempt::enable();
        }

        // We don't need to keep the stack around.
        forget(stack);
    }

    /// # Safety
    /// This function must not be called inside of the scheulder context.
    ///
    /// The caller must ensure that `preempt::count` == 1.
    pub unsafe fn go_from_scheduler(to: &ExecutionContext) {
        // SAFETY: Preemption is disabled.
        unsafe { LOCAL_SCHEDULER_CONTEXT.as_ref() }.switch_to(to);
    }

    /// # Safety
    /// This function must not be called inside of the scheulder context.
    ///
    /// The caller must ensure that `preempt::count` == 1.
    pub unsafe fn goto_scheduler(from: &ExecutionContext) {
        // SAFETY: Preemption is disabled.
        from.switch_to(unsafe { LOCAL_SCHEDULER_CONTEXT.as_ref() });
    }

    /// # Safety
    /// This function must not be called inside of the scheulder context.
    ///
    /// The caller must ensure that `preempt::count` == 1.
    pub unsafe fn goto_scheduler_noreturn() -> ! {
        // SAFETY: Preemption is disabled.
        unsafe { LOCAL_SCHEDULER_CONTEXT.as_ref().switch_noreturn() }
    }

    fn add_task(task: Arc<Task>) {
        TASKS.lock().insert(task);
    }

    fn remove_task(task: &Task) {
        unsafe { TASKS.lock().cursor_mut_from_ptr(task as *const _).remove() };
    }

    fn select_cpu_for_task(&self, task: &Task) -> usize {
        task.cpu.load(Ordering::Relaxed) as _
    }

    pub fn activate(&self, task: &Arc<Task>) {
        // Only one cpu can be activating the task at a time.
        // TODO: Add some checks.

        if task.on_rq.swap(true, Ordering::Acquire) {
            // Lock the rq and check whether the task is on the rq again.
            let cpuid = task.cpu.load(Ordering::Acquire);
            let mut rq = cpu_rq(cpuid as _).lock_irq();

            if !task.on_rq.load(Ordering::Acquire) {
                // Task has just got off the rq. Put it back.
                rq.put(task.clone());
            } else {
                // Task is already on the rq. Do nothing.
                return;
            }
        } else {
            // Task not on some rq. Select one and put it here.
            let cpu = self.select_cpu_for_task(&task);
            let mut rq = cpu_rq(cpu).lock_irq();
            task.cpu.store(cpu as _, Ordering::Release);
            rq.put(task.clone());
        }
    }

    pub fn spawn<S, R>(&self, runnable: R) -> JoinHandle<R::Output>
    where
        S: Stack + 'static,
        R: Run + Contexted + Send + 'static,
        R::Output: Send + 'static,
    {
        let TaskHandle {
            task,
            output_handle,
        } = Task::new::<S, _>(runnable);

        Self::add_task(task.clone());
        self.activate(&task);

        JoinHandle(output_handle)
    }

    /// Go to idle task. Call this with `preempt_count == 1`.
    /// The preempt count will be decremented by this function.
    ///
    /// # Safety
    /// We might never return from here.
    /// Drop all variables that take ownership of some resource before calling this function.
    pub fn schedule() {
        assert_preempt_count_eq!(1, "Scheduler::schedule");

        // Make sure all works are done before scheduling.
        compiler_fence(Ordering::SeqCst);

        // TODO!!!!!: Use of reference here needs further consideration.
        //
        // Since we might never return to here, we can't take ownership of `current()`.
        // Is it safe to believe that `current()` will never change across calls?
        unsafe {
            // SAFETY: Preemption is disabled.
            Scheduler::goto_scheduler(&Task::current().execution_context);
        }
        eonix_preempt::enable();
    }
}

extern "C" fn local_scheduler() -> ! {
    loop {
        assert_preempt_count_eq!(1, "Scheduler::idle_task");
        let mut rq = local_rq().lock_irq();

        let previous_task = CURRENT_TASK
            .get()
            .map(|ptr| unsafe { Arc::from_raw(ptr.as_ptr()) });
        let next_task = rq.get();

        match (previous_task, next_task) {
            (None, None) => {
                // Nothing to do, halt the cpu and rerun the loop.
                drop(rq);
                arch::halt();
                continue;
            }
            (None, Some(next)) => {
                CURRENT_TASK.set(NonNull::new(Arc::into_raw(next) as *mut _));
            }
            (Some(previous), None) => {
                if previous.state.is_running() {
                    // Previous thread is `Running`, return to the current running thread.
                    println_trace!(
                        "trace_scheduler",
                        "Returning to task id({}) without doing context switch",
                        previous.id
                    );
                    CURRENT_TASK.set(NonNull::new(Arc::into_raw(previous) as *mut _));
                } else {
                    // Nothing to do, halt the cpu and rerun the loop.
                    CURRENT_TASK.set(NonNull::new(Arc::into_raw(previous) as *mut _));
                    drop(rq);
                    arch::halt();
                    continue;
                }
            }
            (Some(previous), Some(next)) => {
                println_trace!(
                    "trace_scheduler",
                    "Switching from task id({}) to task id({})",
                    previous.id,
                    next.id
                );

                debug_assert_ne!(previous.id, next.id, "Switching to the same task");

                if previous.state.is_running() || !previous.state.try_park() {
                    rq.put(previous);
                } else {
                    previous.on_rq.store(false, Ordering::Release);
                }

                CURRENT_TASK.set(NonNull::new(Arc::into_raw(next) as *mut _));
            }
        }

        drop(rq);
        // TODO: We can move the release of finished tasks to some worker thread.
        if let ExecuteStatus::Finished = Task::current().run() {
            let current = CURRENT_TASK
                .swap(None)
                .map(|ptr| unsafe { Arc::from_raw(ptr.as_ptr()) })
                .expect("Current task should be present");
            Scheduler::remove_task(&current);
        }
    }
}
