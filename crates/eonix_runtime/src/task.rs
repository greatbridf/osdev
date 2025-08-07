mod adapter;
mod task_state;

use crate::{
    executor::{Executor, OutputHandle},
    ready_queue::{cpu_rq, ReadyQueue},
};
use alloc::{sync::Arc, task::Wake};
use atomic_unique_refcell::AtomicUniqueRefCell;
use core::{
    ops::DerefMut,
    sync::atomic::{AtomicU32, Ordering},
    task::{Context, Poll, Waker},
};
use eonix_hal::processor::CPU;
use eonix_sync::{Spin, SpinIrq};
use intrusive_collections::{LinkedListAtomicLink, RBTreeAtomicLink};

pub use adapter::{TaskAdapter, TaskRqAdapter};
pub(crate) use task_state::TaskState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskId(u32);

pub struct TaskHandle<Output>
where
    Output: Send,
{
    pub(crate) task: Arc<Task>,
    pub(crate) output_handle: Arc<Spin<OutputHandle<Output>>>,
}

pub struct Task {
    /// Unique identifier of the task.
    pub id: TaskId,
    /// The last cpu that the task was executed on.
    /// If `on_rq` is `false`, we can't assume that this task is still on the cpu.
    pub(crate) cpu: AtomicU32,
    /// Task state.
    pub(crate) state: TaskState,
    /// Executor object.
    executor: AtomicUniqueRefCell<Executor>,
    /// Link in the global task list.
    link_task_list: RBTreeAtomicLink,
    /// Link in the ready queue.
    link_ready_queue: LinkedListAtomicLink,
}

impl Task {
    pub fn new<F>(future: F) -> TaskHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        static ID: AtomicU32 = AtomicU32::new(0);

        let (executor, output_handle) = Executor::new(future);

        let task = Arc::new(Self {
            id: TaskId(ID.fetch_add(1, Ordering::Relaxed)),
            cpu: AtomicU32::new(CPU::local().cpuid() as u32),
            state: TaskState::new(TaskState::BLOCKED),
            executor: AtomicUniqueRefCell::new(executor),
            link_task_list: RBTreeAtomicLink::new(),
            link_ready_queue: LinkedListAtomicLink::new(),
        });

        TaskHandle {
            task,
            output_handle,
        }
    }

    pub fn poll(self: &Arc<Self>) -> Poll<()> {
        let mut executor_borrow = self.executor.borrow();
        let waker = Waker::from(self.clone());
        let mut cx = Context::from_waker(&waker);

        executor_borrow.poll(&mut cx)
    }

    /// Get the stabilized lock for the task's run queue.
    pub fn rq(&self) -> impl DerefMut<Target = dyn ReadyQueue> + 'static {
        loop {
            let cpu = self.cpu.load(Ordering::Relaxed);
            let rq = cpu_rq(cpu as usize).lock_irq();

            // We stabilize the task cpu with the cpu rq here for now.
            if cpu != self.cpu.load(Ordering::Acquire) {
                continue;
            }

            return rq;
        }
    }

    pub fn block_on<F>(future: F) -> F::Output
    where
        F: Future,
    {
        todo!()
    }
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let Ok(old) = self.state.update(|state| match state {
            TaskState::BLOCKED => Some(TaskState::READY),
            TaskState::RUNNING => Some(TaskState::READY | TaskState::RUNNING),
            TaskState::READY | TaskState::READY_RUNNING => None,
            state => unreachable!("Waking a {state:?} task"),
        }) else {
            return;
        };

        if old == TaskState::BLOCKED {
            // If the task was blocked, we need to put it back to the ready queue.
            self.rq().put(self.clone());
        }
    }
}
