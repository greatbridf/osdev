mod adapter;
mod task_state;
mod wait_list;

use crate::{
    context::ExecutionContext,
    executor::{ExecuteStatus, Executor, ExecutorBuilder, OutputHandle, Stack},
    run::{Contexted, Run},
    scheduler::Scheduler,
};
use alloc::{boxed::Box, sync::Arc, task::Wake};
use atomic_unique_refcell::AtomicUniqueRefCell;
use core::{
    pin::Pin,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
    task::Waker,
};
use eonix_sync::Spin;
use intrusive_collections::RBTreeAtomicLink;
use task_state::TaskState;

pub use adapter::TaskAdapter;
pub use wait_list::TaskWait;

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
/// ## Task Sleeping and Waking up
///
/// ### Waiters
///
/// lock => check condition no => save waker => set state sleep => unlock => return pending
///
/// executor check state -> if sleeping => goto scheduler => get rq lock => scheduler check state
///
///                                                                      -> if sleeping => on_rq = false
///
///                                                                      -> if running => enqueue
///
///                      -> if running => poll again
///
/// ### Wakers
///
/// lock => set condition yes => get waker => unlock => if has waker
///
/// set state running => swap on_rq true => get rq lock => check on_rq true again => if false enqueue
pub struct Task {
    /// Unique identifier of the task.
    pub id: TaskId,
    /// Whether the task is on some run queue (a.k.a ready).
    pub(crate) on_rq: AtomicBool,
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

impl<Output> TaskHandle<Output>
where
    Output: Send,
{
    pub fn waker(&self) -> Waker {
        Waker::from(self.task.clone())
    }
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
            cpu: AtomicU32::new(0),
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

    /// Temporary solution.
    pub unsafe fn sleep(&self) {
        self.state.swap(TaskState::SLEEPING);
    }
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        // TODO: Check the fast path where we're waking up current.

        // SAFETY: All the operations below should happen after we've read the sleeping state.
        let old_state = self.state.swap(TaskState::RUNNING);
        if old_state != TaskState::SLEEPING {
            return;
        }

        // If we get here, we should be the only one waking up the task.
        Scheduler::get().activate(self);
    }
}
