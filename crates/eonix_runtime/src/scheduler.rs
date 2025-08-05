use crate::{
    executor::OutputHandle,
    ready_queue::{cpu_rq, local_rq, ReadyQueue},
    task::{Task, TaskAdapter, TaskHandle, TaskState},
};
use alloc::sync::Arc;
use core::{
    ops::{Deref, DerefMut},
    ptr::NonNull,
    sync::atomic::Ordering,
    task::Waker,
};
use eonix_hal::processor::halt;
use eonix_sync::{LazyLock, Spin, SpinIrq as _};
use intrusive_collections::RBTree;
use pointers::BorrowedArc;

#[eonix_percpu::define_percpu]
static CURRENT_TASK: Option<NonNull<Task>> = None;

static TASKS: LazyLock<Spin<RBTree<TaskAdapter>>> =
    LazyLock::new(|| Spin::new(RBTree::new(TaskAdapter::new())));

pub static RUNTIME: Runtime = Runtime();

pub struct Runtime();

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

impl Runtime {
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

    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let TaskHandle {
            task,
            output_handle,
        } = Task::new(future);

        self.add_task(task.clone());
        self.activate(&task);

        JoinHandle(output_handle)
    }

    // /// Go to idle task. Call this with `preempt_count == 1`.
    // /// The preempt count will be decremented by this function.
    // ///
    // /// # Safety
    // /// We might never return from here.
    // /// Drop all variables that take ownership of some resource before calling this function.
    // pub fn schedule() {
    //     assert_preempt_count_eq!(1, "Scheduler::schedule");

    //     // Make sure all works are done before scheduling.
    //     compiler_fence(Ordering::SeqCst);

    //     // TODO!!!!!: Use of reference here needs further consideration.
    //     //
    //     // Since we might never return to here, we can't take ownership of `current()`.
    //     // Is it safe to believe that `current()` will never change across calls?
    //     unsafe {
    //         // SAFETY: Preemption is disabled.
    //         Scheduler::goto_scheduler(&Task::current().execution_context);
    //     }
    //     eonix_preempt::enable();
    // }
}

impl Runtime {
    fn add_task(&self, task: Arc<Task>) {
        TASKS.lock_irq().insert(task);
    }

    fn remove_task(&self, task: &impl Deref<Target = Arc<Task>>) {
        unsafe {
            TASKS
                .lock_irq()
                .cursor_mut_from_ptr(Arc::as_ptr(task))
                .remove();
        }
    }

    fn current(&self) -> Option<BorrowedArc<Task>> {
        CURRENT_TASK
            .get()
            .map(|ptr| unsafe { BorrowedArc::from_raw(ptr) })
    }

    fn remove_and_enqueue_current(&self, rq: &mut impl DerefMut<Target = dyn ReadyQueue>) {
        let Some(current) = self.current() else {
            return;
        };

        match current.state.cmpxchg(TaskState::RUNNING, TaskState::READY) {
            Ok(_) => {
                let current = unsafe {
                    Arc::from_raw(
                        CURRENT_TASK
                            .get()
                            .expect("Current task should be present")
                            .as_ptr(),
                    )
                };

                rq.put(current);
            }
            Err(old) => {
                assert_eq!(
                    old,
                    TaskState::PARKED,
                    "Current task should be in PARKED state"
                );
            }
        }
    }

    /// Enter the runtime with an "init" future and run till its completion.
    ///
    /// The "init" future has the highest priority and when it completes,
    /// the runtime will exit immediately and yield its output.
    pub fn enter(&self) {
        loop {
            let mut rq = local_rq().lock_irq();

            self.remove_and_enqueue_current(&mut rq);

            let Some(next) = rq.get() else {
                drop(rq);
                halt();
                continue;
            };

            let old_state = next.state.swap(TaskState::RUNNING);
            assert_eq!(
                old_state,
                TaskState::READY,
                "Next task should be in READY state"
            );

            CURRENT_TASK.set(NonNull::new(Arc::into_raw(next) as *mut _));
            drop(rq);

            // TODO: MAYBE we can move the release of finished tasks to some worker thread.
            if Task::current().run() {
                Task::current().state.set(TaskState::DEAD);
                CURRENT_TASK.set(None);

                self.remove_task(&Task::current());
            }
        }
    }
}
