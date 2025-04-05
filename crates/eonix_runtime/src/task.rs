mod adapter;
mod task_state;

use crate::{
    context::ExecutionContext,
    executor::{ExecuteStatus, Executor, ExecutorBuilder, OutputHandle, Stack},
    run::{Contexted, PinRun},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskId(u32);

pub struct UniqueWaker(Arc<Task>);

pub struct TaskHandle<Output>
where
    Output: Send,
{
    pub(crate) task: Arc<Task>,
    pub(crate) output_handle: Arc<Spin<OutputHandle<Output>>>,
}

/// A `Task` represents a schedulable unit.
pub struct Task {
    /// Unique identifier of the task.
    pub id: TaskId,
    /// Whether the task is on some run queue.
    pub(crate) on_rq: AtomicBool,
    /// Task execution context.
    pub(crate) execution_context: ExecutionContext,
    /// Executor object.
    executor: AtomicUniqueRefCell<Option<Pin<Box<dyn Executor>>>>,
    /// Task state.
    state: TaskState,
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
        R: PinRun + Contexted + Send + 'static,
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
            executor: AtomicUniqueRefCell::new(Some(executor)),
            execution_context,
            state: TaskState::new(TaskState::RUNNING),
            link_task_list: RBTreeAtomicLink::new(),
        });

        TaskHandle {
            task,
            output_handle: output,
        }
    }

    pub fn is_runnable(&self) -> bool {
        self.state.is_runnable()
    }

    pub(super) fn set_usleep(&self) {
        let prev_state = self.state.swap(TaskState::USLEEP);
        assert_eq!(
            prev_state,
            TaskState::RUNNING,
            "Trying to set task {} usleep that is not running",
            self.id.0
        );
    }

    pub fn usleep(self: &Arc<Self>) -> Arc<UniqueWaker> {
        // No need to dequeue. We have proved that the task is running so not in the queue.
        self.set_usleep();

        Arc::new(UniqueWaker(self.clone()))
    }

    pub fn isleep(self: &Arc<Self>) -> Arc<Self> {
        // No need to dequeue. We have proved that the task is running so not in the queue.
        let prev_state = self.state.cmpxchg(TaskState::RUNNING, TaskState::ISLEEP);

        assert_eq!(
            prev_state,
            TaskState::RUNNING,
            "Trying to sleep task {} that is not running",
            self.id.0
        );

        self.clone()
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
            self.set_usleep();
            ExecuteStatus::Finished
        } else {
            ExecuteStatus::Executing
        }
    }
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        match self.state.cmpxchg(TaskState::ISLEEP, TaskState::RUNNING) {
            TaskState::RUNNING | TaskState::USLEEP => return,
            TaskState::ISLEEP => Scheduler::get().activate(self),
            state => panic!("Invalid transition from state {:?} to `Running`", state),
        }
    }
}

impl Wake for UniqueWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let Self(task) = &**self;

        let prev_state = task.state.swap(TaskState::RUNNING);
        assert_eq!(prev_state, TaskState::USLEEP);

        Scheduler::get().activate(task);
    }
}
