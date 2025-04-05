use crate::{
    context::ExecutionContext,
    executor::{ExecuteStatus, OutputHandle, Stack},
    ready_queue::{init_local_rq, local_rq, ReadyQueue},
    run::{Contexted, PinRun},
    task::{Task, TaskAdapter, TaskHandle},
};
use alloc::sync::Arc;
use core::{
    future::Future,
    mem::forget,
    pin::Pin,
    ptr::NonNull,
    sync::atomic::{compiler_fence, Ordering},
    task::{Context, Poll, Waker},
};
use eonix_log::println_trace;
use eonix_preempt::assert_preempt_count_eq;
use eonix_sync::Spin;
use intrusive_collections::RBTree;
use lazy_static::lazy_static;
use pointers::BorrowedArc;

#[arch::define_percpu]
static CURRENT_TASK: Option<NonNull<Task>> = None;

#[arch::define_percpu]
static LOCAL_SCHEDULER_CONTEXT: ExecutionContext = ExecutionContext::new();

lazy_static! {
    static ref TASKS: Spin<RBTree<TaskAdapter>> = Spin::new(RBTree::new(TaskAdapter::new()));
}

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
        init_local_rq();

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

    fn select_rq_for_task(&self, _task: &Task) -> &'static Spin<dyn ReadyQueue> {
        // TODO: Select an appropriate ready queue.
        local_rq()
    }

    pub fn activate(&self, task: &Arc<Task>) {
        if !task.on_rq.swap(true, Ordering::AcqRel) {
            let rq = self.select_rq_for_task(&task);
            rq.lock_irq().put(task.clone());
        }
    }

    pub fn spawn<S, R>(&self, runnable: R) -> JoinHandle<R::Output>
    where
        S: Stack + 'static,
        R: PinRun + Contexted + Send + 'static,
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

    pub async fn yield_now() {
        struct Yield(bool);

        impl Future for Yield {
            type Output = ();

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                match *self {
                    Yield(true) => Poll::Ready(()),
                    Yield(false) => {
                        self.set(Yield(true));
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                }
            }
        }

        Yield(false).await
    }

    pub async fn sleep() {
        struct Sleep(bool);

        impl Future for Sleep {
            type Output = ();

            fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
                match *self {
                    Sleep(true) => Poll::Ready(()),
                    Sleep(false) => {
                        self.set(Sleep(true));
                        Poll::Pending
                    }
                }
            }
        }

        Sleep(false).await
    }
}

extern "C" fn local_scheduler() -> ! {
    loop {
        assert_preempt_count_eq!(1, "Scheduler::idle_task");
        let previous_task = CURRENT_TASK
            .get()
            .map(|ptr| unsafe { Arc::from_raw(ptr.as_ptr()) });
        let next_task = local_rq().lock().get();

        match (previous_task, next_task) {
            (None, None) => {
                // Nothing to do, halt the cpu and rerun the loop.
                arch::halt();
                continue;
            }
            (None, Some(next)) => {
                CURRENT_TASK.set(NonNull::new(Arc::into_raw(next) as *mut _));
            }
            (Some(previous), None) if previous.is_runnable() => {
                // Previous thread is `Running`, return to the current running thread.
                println_trace!(
                    "trace_scheduler",
                    "Returning to task id({}) without doing context switch",
                    previous.id
                );

                CURRENT_TASK.set(NonNull::new(Arc::into_raw(previous) as *mut _));
            }
            (Some(previous), None) => {
                // Nothing to do, halt the cpu and rerun the loop.
                CURRENT_TASK.set(NonNull::new(Arc::into_raw(previous) as *mut _));
                arch::halt();
                continue;
            }
            (Some(previous), Some(next)) => {
                println_trace!(
                    "trace_scheduler",
                    "Switching from task id({}) to task id({})",
                    previous.id,
                    next.id
                );

                debug_assert_ne!(previous.id, next.id, "Switching to the same task");

                let mut rq = local_rq().lock();
                if previous.is_runnable() {
                    rq.put(previous);
                } else {
                    // TODO!!!!!!!!!: There is a race condition here if we reach here and there
                    // is another thread waking the task up. They might read `on_rq` == true so
                    // the task will never be waken up.
                    previous.on_rq.store(false, Ordering::Release);
                }

                CURRENT_TASK.set(NonNull::new(Arc::into_raw(next) as *mut _));
            }
        }

        // TODO: We can move the release of finished tasks to some worker thread.
        if let ExecuteStatus::Finished = Task::current().run() {
            Scheduler::remove_task(&Task::current());
        }
    }
}
