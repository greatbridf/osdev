use core::{
    future::Future,
    pin::Pin,
    ptr::NonNull,
    sync::atomic::{compiler_fence, Ordering},
    task::{Context, Poll, Waker},
};

use crate::{kernel::console::println_trace, prelude::*, sync::preempt};

use alloc::sync::Arc;

use intrusive_collections::RBTree;
use lazy_static::lazy_static;

use super::{
    init_rq_thiscpu,
    readyqueue::rq_thiscpu,
    task::{FutureRunnable, TaskAdapter, TaskHandle, TaskOutput},
    Task,
};

pub struct Scheduler;

pub struct JoinHandle<Output>(Arc<Spin<TaskOutput<Output>>>)
where
    Output: Send;

/// Idle task
/// All the idle tasks are pinned to the current cpu.
#[arch::define_percpu]
static IDLE_TASK: Option<NonNull<Task>> = None;

/// Current running task
#[arch::define_percpu]
static CURRENT: Option<NonNull<Task>> = None;

lazy_static! {
    static ref TASKS: Spin<RBTree<TaskAdapter>> = Spin::new(RBTree::new(TaskAdapter::new()));
}

impl Task {
    /// # Safety
    /// We should never "inspect" a change in `current`.
    /// The change of `CURRENT` will only happen in the scheduler. And if we are preempted,
    /// when we DO return, the `CURRENT` will be the same and remain valid.
    pub fn current<'a>() -> BorrowedArc<'a, Task> {
        BorrowedArc::from_raw(CURRENT.get().unwrap().as_ptr())
    }

    /// # Safety
    /// Idle task should never change so we can borrow it without touching the refcount.
    pub fn idle() -> BorrowedArc<'static, Task> {
        BorrowedArc::from_raw(IDLE_TASK.get().unwrap().as_ptr())
    }

    pub fn add(task: Arc<Self>) {
        TASKS.lock().insert(task);
    }

    pub fn remove(&self) {
        unsafe { TASKS.lock().cursor_mut_from_ptr(self as *const _) }.remove();
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

    pub fn init_scheduler_thiscpu() {
        let runnable = FutureRunnable::new(idle_task());
        let (init_task, _) = Self::extract_handle(Task::new(runnable));
        TASKS.lock().insert(init_task.clone());

        init_rq_thiscpu();
        Self::set_idle_and_current(init_task);
    }

    pub fn set_idle_and_current(task: Arc<Task>) {
        task.set_usleep();

        let old = IDLE_TASK.swap(NonNull::new(Arc::into_raw(task.clone()) as *mut _));
        assert!(old.is_none(), "Idle task is already set");

        let old = CURRENT.swap(NonNull::new(Arc::into_raw(task) as *mut _));
        assert!(old.is_none(), "Current is already set");
    }

    pub fn activate(&self, task: &Arc<Task>) {
        // TODO: Select an appropriate ready queue to enqueue.
        if !task.on_rq.swap(true, Ordering::AcqRel) {
            rq_thiscpu().lock_irq().put(task.clone());
        }
    }

    pub fn spawn<O>(&self, task: TaskHandle<O>) -> JoinHandle<O>
    where
        O: Send,
    {
        let (task, output) = Self::extract_handle(task);
        Task::add(task.clone());
        self.activate(&task);

        JoinHandle(output)
    }

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
        Task::switch(&Task::current(), &Task::idle());
        preempt::enable();
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

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
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

async fn idle_task() {
    preempt::disable();
    let mut cx = Context::from_waker(Waker::noop());

    loop {
        debug_assert_eq!(
            preempt::count(),
            1,
            "Scheduler::idle_task() preempt count != 1"
        );

        let next = rq_thiscpu().lock().get();
        match next {
            None if Task::current().is_runnable() => {
                println_trace!(
                    "trace_scheduler",
                    "Returning to task id({}) without doing context switch",
                    Task::current().id
                );

                // Previous thread is `Running`, return to the current running thread.
                Task::current().run(&mut cx);
            }
            None => {
                // Halt the cpu and rerun the loop.
                arch::halt();
            }
            Some(next) => {
                println_trace!(
                    "trace_scheduler",
                    "Switching from task id({}) to task id({})",
                    Task::current().id,
                    next.id
                );

                debug_assert_ne!(next.id, Task::current().id, "Switching to the same task");

                if let Some(task_pointer) =
                    CURRENT.swap(NonNull::new(Arc::into_raw(next) as *mut _))
                {
                    let task = unsafe { Arc::from_raw(task_pointer.as_ptr()) };
                    let mut rq = rq_thiscpu().lock();

                    if task.is_runnable() {
                        rq.put(task);
                    } else {
                        // TODO!!!!!!!!!: There is a race condition here if we reach here and there
                        // is another thread waking the task up. They might read `on_rq` == true so
                        // the task will never be waken up.
                        task.on_rq.store(false, Ordering::Release);
                    }
                }

                Task::current().run(&mut cx);
            }
        }
    }
}
