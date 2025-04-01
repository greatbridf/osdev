mod context;
mod kstack;
mod runnable;

pub use context::TaskContext;
pub use runnable::{Contexted, PinRunnable, RunState};

use atomic_unique_refcell::AtomicUniqueRefCell;
use kstack::KernelStack;

use core::{
    future::Future,
    pin::Pin,
    sync::atomic::{fence, AtomicBool, AtomicU32, Ordering},
    task::{Context, Poll, Waker},
};

use alloc::{
    boxed::Box,
    sync::{Arc, Weak},
    task::Wake,
};
use intrusive_collections::{intrusive_adapter, KeyAdapter, RBTreeAtomicLink};

use crate::{kernel::task::Scheduler, sync::preempt, Spin};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskId(u32);

#[derive(Debug)]
pub struct TaskState(AtomicU32);

pub struct UniqueWaker(Arc<Task>);

pub struct TaskHandle<Output: Send> {
    /// The task itself.
    task: Arc<Task>,
    /// The output of the task.
    output: Arc<Spin<TaskOutput<Output>>>,
}

enum TaskOutputState<Output: Send> {
    Waiting(Option<Waker>),
    Finished(Option<Output>),
    TakenOut,
}

pub struct TaskOutput<Output: Send> {
    inner: TaskOutputState<Output>,
}

impl<Output> TaskOutput<Output>
where
    Output: Send,
{
    pub fn try_resolve(&mut self) -> Option<Output> {
        let output = match &mut self.inner {
            TaskOutputState::Waiting(_) => return None,
            TaskOutputState::Finished(output) => output.take(),
            TaskOutputState::TakenOut => panic!("Output already taken out"),
        };

        self.inner = TaskOutputState::TakenOut;
        if let Some(output) = output {
            Some(output)
        } else {
            unreachable!("Output should be present")
        }
    }

    pub fn register_waiter(&mut self, waker: Waker) {
        if let TaskOutputState::Waiting(inner_waker) = &mut self.inner {
            inner_waker.replace(waker);
        } else {
            panic!("Output is not waiting");
        }
    }

    pub fn commit_output(&mut self, output: Output) {
        if let TaskOutputState::Waiting(inner_waker) = &mut self.inner {
            if let Some(waker) = inner_waker.take() {
                waker.wake();
            }
            self.inner = TaskOutputState::Finished(Some(output));
        } else {
            panic!("Output is not waiting");
        }
    }
}

/// A `Task` represents a schedulable unit.
pub struct Task {
    /// Unique identifier of the task.
    pub id: TaskId,
    /// Whether the task is on some run queue.
    pub(super) on_rq: AtomicBool,
    /// Executor object.
    executor: AtomicUniqueRefCell<Option<Pin<Box<dyn Future<Output = ()> + Send>>>>,
    /// Task execution context.
    task_context: TaskContext,
    /// Task state.
    state: TaskState,
    /// Link in the global task list.
    link_task_list: RBTreeAtomicLink,
}

intrusive_adapter!(pub TaskAdapter = Arc<Task>: Task { link_task_list: RBTreeAtomicLink });
impl<'a> KeyAdapter<'a> for TaskAdapter {
    type Key = TaskId;
    fn get_key(&self, task: &'a Task) -> Self::Key {
        task.id
    }
}

impl Scheduler {
    pub(super) fn extract_handle<O>(handle: TaskHandle<O>) -> (Arc<Task>, Arc<Spin<TaskOutput<O>>>)
    where
        O: Send,
    {
        let TaskHandle { task, output } = handle;
        (task, output)
    }
}

impl TaskState {
    pub const RUNNING: u32 = 0;
    pub const ISLEEP: u32 = 1;
    pub const USLEEP: u32 = 2;

    pub const fn new(state: u32) -> Self {
        Self(AtomicU32::new(state))
    }

    pub fn swap(&self, state: u32) -> u32 {
        self.0.swap(state, Ordering::AcqRel)
    }

    pub fn cmpxchg(&self, current: u32, new: u32) -> u32 {
        self.0
            .compare_exchange(current, new, Ordering::AcqRel, Ordering::Acquire)
            .unwrap_or_else(|x| x)
    }

    pub fn is_runnable(&self) -> bool {
        self.0.load(Ordering::Acquire) == Self::RUNNING
    }
}

impl Task {
    pub fn new<R, O>(runnable: R) -> TaskHandle<R::Output>
    where
        O: Send,
        R: PinRunnable<Output = O> + Contexted + Send + 'static,
    {
        static ID: AtomicU32 = AtomicU32::new(0);

        let output = Arc::new(Spin::new(TaskOutput {
            inner: TaskOutputState::Waiting(None),
        }));

        let kernel_stack = KernelStack::new();
        let mut task_context = TaskContext::new();
        task_context.set_sp(kernel_stack.get_stack_bottom());

        let mut executor = Box::pin(Executor::new(kernel_stack, runnable));

        task_context.call2(
            Self::_executor::<O, R>,
            [
                unsafe { executor.as_mut().get_unchecked_mut() } as *mut _ as _,
                Weak::into_raw(Arc::downgrade(&output)) as usize,
            ],
        );

        let task = Arc::new(Self {
            id: TaskId(ID.fetch_add(1, Ordering::Relaxed)),
            on_rq: AtomicBool::new(false),
            executor: AtomicUniqueRefCell::new(Some(executor)),
            task_context,
            state: TaskState::new(TaskState::RUNNING),
            link_task_list: RBTreeAtomicLink::new(),
        });

        TaskHandle { task, output }
    }

    pub fn is_runnable(&self) -> bool {
        self.state.is_runnable()
    }

    pub(super) fn set_usleep(&self) {
        let prev_state = self.state.swap(TaskState::USLEEP);
        assert_eq!(prev_state, TaskState::RUNNING);
    }

    pub fn usleep(self: &Arc<Self>) -> Arc<UniqueWaker> {
        // No need to dequeue. We have proved that the task is running so not in the queue.
        self.set_usleep();

        Arc::new(UniqueWaker(self.clone()))
    }

    pub fn isleep(self: &Arc<Self>) -> Arc<Self> {
        // No need to dequeue. We have proved that the task is running so not in the queue.
        let prev_state = self.state.swap(TaskState::ISLEEP);
        assert_eq!(prev_state, TaskState::RUNNING);

        self.clone()
    }

    pub fn switch(from: &Self, to: &Self) {
        from.task_context.switch_to(&to.task_context);
    }

    pub fn switch_noreturn(to: &Self) -> ! {
        to.task_context.switch_noreturn();
    }

    unsafe extern "C" fn _executor<O, R>(
        executor: Pin<&mut Executor<R>>,
        output: *const Spin<TaskOutput<R::Output>>,
    ) -> !
    where
        O: Send,
        R: PinRunnable<Output = O> + Send + Contexted,
    {
        // We get here with preempt count == 1.
        preempt::enable();

        let output = Weak::from_raw(output);
        let executor = unsafe { executor.get_unchecked_mut() };
        let runnable = unsafe { Pin::new_unchecked(&mut executor.runnable) };

        {
            let waker = Waker::from(Task::current().clone());
            let output_data = runnable.pinned_join(&waker);

            if let Some(output) = output.upgrade() {
                output.lock().commit_output(output_data);
            }
        }

        // SAFETY: We are on the same CPU as the task.
        executor.finished.store(true, Ordering::Relaxed);

        // Idle task needs preempt count == 1.
        preempt::disable();
        Task::switch_noreturn(&Task::idle());
    }

    pub fn run(&self, cx: &mut Context) {
        let mut executor = self.executor.borrow();
        let real_executor = executor.as_mut().expect("Executor should be present");

        if let Poll::Ready(_) = real_executor.as_mut().poll(cx) {
            executor.take();
            self.set_usleep();
            Self::remove(self);
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

struct Executor<R>
where
    R: PinRunnable + Send + Contexted + 'static,
{
    _kernel_stack: KernelStack,
    runnable: R,
    finished: AtomicBool,
}

impl<R> Executor<R>
where
    R: PinRunnable + Send + Contexted + 'static,
{
    pub fn new(kernel_stack: KernelStack, runnable: R) -> Self {
        Self {
            _kernel_stack: kernel_stack,
            runnable,
            finished: AtomicBool::new(false),
        }
    }
}

impl<R> Future for Executor<R>
where
    R: PinRunnable + Send + Contexted + 'static,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        // TODO!!!: We should load the context only if the previous task is
        // different from the current task.

        // SAFETY: We don't move the runnable object.
        let executor = unsafe { self.get_unchecked_mut() };
        executor.runnable.load_running_context();

        // TODO!!!: If the task comes from another cpu, we need to sync.
        //
        // The other cpu should see the changes of kernel stack of the target thread
        // made in this cpu.
        //
        // Can we find a better way other than `fence`s?
        //
        // An alternative way is to use an atomic variable to store the cpu id of
        // the current task. Then we can use acquire release swap to ensure that the
        // other cpu sees the changes.
        fence(Ordering::SeqCst);

        Task::switch(&Task::idle(), &Task::current());

        fence(Ordering::SeqCst);

        if executor.finished.load(Ordering::Relaxed) {
            return Poll::Ready(());
        }

        return Poll::Pending;
    }
}

pub struct FutureRunnable<F: Future>(F);

impl<F> FutureRunnable<F>
where
    F: Future,
{
    pub const fn new(future: F) -> Self {
        Self(future)
    }
}

impl<F: Future + 'static> Contexted for FutureRunnable<F> {
    fn load_running_context(&mut self) {}
}

impl<F: Future + 'static> PinRunnable for FutureRunnable<F> {
    type Output = F::Output;

    fn pinned_run(self: Pin<&mut Self>, waker: &Waker) -> RunState<Self::Output> {
        let mut future = unsafe { self.map_unchecked_mut(|me| &mut me.0) };
        let mut context = Context::from_waker(waker);

        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => RunState::Finished(output),
            Poll::Pending => RunState::Running,
        }
    }
}
