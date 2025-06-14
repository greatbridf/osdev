mod adapter;
mod task_state;

use crate::{
    context::ExecutionContext,
    executor::{ExecuteStatus, Executor, ExecutorBuilder, OutputHandle, Stack},
    run::{Contexted, Run},
    scheduler::Scheduler,
};
use alloc::{boxed::Box, sync::Arc, task::Wake};
use atomic_unique_refcell::AtomicUniqueRefCell;
use core::{
    pin::{pin, Pin},
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
    task::{Context, Poll, Waker},
};
use eonix_hal::processor::CPU;
use eonix_preempt::assert_preempt_enabled;
use eonix_sync::Spin;
use intrusive_collections::RBTreeAtomicLink;
use task_state::TaskState;

pub use adapter::TaskAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskId(u32);

pub struct TaskHandle<Output>
where
    Output: Send,
{
    pub(crate) task: Arc<Task>,
    pub(crate) output_handle: Arc<Spin<OutputHandle<Output>>>,
}

/// A `Task` represents a schedulable unit.
///
/// Initial: state = Running, unparked = false
///
/// Task::park() => swap state <- Parking, assert prev == Running
///              => swap unparked <- false
///              -> true => store state <- Running => return
///              -> false => goto scheduler => get rq lock => load state
///                                                        -> Running => enqueue
///                                                        -> Parking => cmpxchg Parking -> Parked
///                                                                   -> Running => enqueue
///                                                                   -> Parking => on_rq <- false
///                                                                   -> Parked => ???
///
/// Task::unpark() => swap unparked <- true
///                -> true => return
///                -> false => swap state <- Running
///                         -> Running => return
///                         -> Parking | Parked => Scheduler::activate
pub struct Task {
    /// Unique identifier of the task.
    pub id: TaskId,
    /// Whether the task is on some run queue (a.k.a ready).
    pub(crate) on_rq: AtomicBool,
    /// Whether someone has called `unpark` on this task.
    pub(crate) unparked: AtomicBool,
    /// The last cpu that the task was executed on.
    /// If `on_rq` is `false`, we can't assume that this task is still on the cpu.
    pub(crate) cpu: AtomicU32,
    /// Task state.
    pub(crate) state: TaskState,
    /// Task execution context.
    pub(crate) execution_context: ExecutionContext,
    /// Executor object.
    executor: AtomicUniqueRefCell<Option<Pin<Box<dyn Executor>>>>,
    /// Link in the global task list.
    link_task_list: RBTreeAtomicLink,
}

impl Task {
    pub fn new<S, R>(runnable: R) -> TaskHandle<R::Output>
    where
        S: Stack + 'static,
        R: Run + Contexted + Send + 'static,
        R::Output: Send + 'static,
    {
        static ID: AtomicU32 = AtomicU32::new(0);

        let (executor, execution_context, output) = ExecutorBuilder::new()
            .stack(S::new())
            .runnable(runnable)
            .build();

        let task = Arc::new(Self {
            id: TaskId(ID.fetch_add(1, Ordering::Relaxed)),
            on_rq: AtomicBool::new(false),
            unparked: AtomicBool::new(false),
            cpu: AtomicU32::new(CPU::local().cpuid() as u32),
            state: TaskState::new(TaskState::RUNNING),
            executor: AtomicUniqueRefCell::new(Some(executor)),
            execution_context,
            link_task_list: RBTreeAtomicLink::new(),
        });

        TaskHandle {
            task,
            output_handle: output,
        }
    }

    pub fn run(&self) -> ExecuteStatus {
        let mut executor_borrow = self.executor.borrow();

        let executor = executor_borrow
            .as_ref()
            .expect("Executor should be present")
            .as_ref()
            .get_ref();

        if let ExecuteStatus::Finished = executor.progress() {
            executor_borrow.take();
            ExecuteStatus::Finished
        } else {
            ExecuteStatus::Executing
        }
    }

    pub fn unpark(self: &Arc<Self>) {
        if self.unparked.swap(true, Ordering::Release) {
            return;
        }

        eonix_preempt::disable();

        match self.state.swap(TaskState::RUNNING) {
            TaskState::RUNNING => {}
            TaskState::PARKED | TaskState::PARKING => {
                // We are waking up from sleep or someone else is parking this task.
                // Try to wake it up.
                Scheduler::get().activate(self);
            }
            _ => unreachable!(),
        }

        eonix_preempt::enable();
    }

    pub fn park() {
        eonix_preempt::disable();
        Self::park_preempt_disabled();
    }

    /// Park the current task with `preempt::count() == 1`.
    pub fn park_preempt_disabled() {
        let task = Task::current();

        let old_state = task.state.swap(TaskState::PARKING);
        assert_eq!(
            old_state,
            TaskState::RUNNING,
            "Parking a task that is not running."
        );

        if task.unparked.swap(false, Ordering::AcqRel) {
            // Someone has called `unpark` on this task previously.
            task.state.swap(TaskState::RUNNING);
        } else {
            unsafe {
                // SAFETY: Preemption is disabled.
                Scheduler::goto_scheduler(&Task::current().execution_context)
            };
            assert!(task.unparked.swap(false, Ordering::Acquire));
        }

        eonix_preempt::enable();
    }

    pub fn block_on<F>(future: F) -> F::Output
    where
        F: Future,
    {
        assert_preempt_enabled!("block_on() must be called with preemption enabled");

        let waker = Waker::from(Task::current().clone());
        let mut context = Context::from_waker(&waker);
        let mut future = pin!(future);

        loop {
            if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
                break output;
            }

            Task::park();
        }
    }
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.unpark();
    }
}
