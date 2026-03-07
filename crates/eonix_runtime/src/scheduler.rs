use alloc::sync::Arc;
use alloc::task::Wake;
use core::hint::assert_unchecked;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use core::task::{Context, Poll, Waker};

use eonix_hal::processor::halt;
use eonix_log::println_trace;
use eonix_sync::{LazyLock, Spin, SpinIrq as _};
use intrusive_collections::RBTree;
use pointers::BorrowedArc;

use crate::executor::OutputHandle;
use crate::ready_queue::{local_rq, ReadyQueue};
use crate::task::{Task, TaskAdapter, TaskHandle, TaskState};

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
            // SAFETY: We should never "inspect" a change in `current`:
            // The change of `CURRENT` will only happen in the scheduler.
            // And if we are preempted outside of the scheduler's context, when
            // we return, `CURRENT` will be the same and remain valid.
            BorrowedArc::from_raw(
                CURRENT_TASK.get().expect("Current task should be present"),
            )
        }
    }

    #[inline(always)]
    fn _swap_current(current: Option<Arc<Task>>) -> Option<Arc<Task>> {
        let current_ptr = current.map(|arc| unsafe {
            // SAFETY: `into_raw` always returns a valid pointer.
            NonNull::new_unchecked(Arc::into_raw(arc) as *mut _)
        });

        let old = CURRENT_TASK.swap(current_ptr);

        old.map(|ptr| unsafe {
            // SAFETY: The pointer always comes from Arc::into_raw
            Arc::from_raw(ptr.as_ptr())
        })
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
        task.wake_by_ref();

        JoinHandle(output_handle)
    }

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

    fn remove_and_enqueue_current(
        &self, rq: &mut impl DerefMut<Target = dyn ReadyQueue>,
    ) {
        let Some(current) = Task::_swap_current(None) else {
            return;
        };

        match current.state.update(|state| match state {
            TaskState::READY_RUNNING => Some(TaskState::READY),
            TaskState::RUNNING => Some(TaskState::BLOCKED),
            _ => {
                unreachable!("Current task should be at least in RUNNING state, but got {state:?}")
            }
        }) {
            Ok(TaskState::READY_RUNNING) => {
                println_trace!(
                    feat: "trace_scheduler",
                    "Re-enqueueing task {:?} (CPU{})",
                    current.id,
                    eonix_hal::processor::CPU::local().cpuid(),
                );

                rq.put(current);
            }
            Ok(_) => {
                println_trace!(
                    feat: "trace_scheduler",
                    "Current task {:?} (CPU{}) is blocked, not re-enqueueing",
                    current.id,
                    eonix_hal::processor::CPU::local().cpuid(),
                );
            }
            _ => unreachable!(),
        }
    }

    pub fn block_till_woken(
        set_waker: impl FnOnce(&Waker),
    ) -> impl Future<Output = ()> {
        struct BlockTillWoken<F: FnOnce(&Waker)> {
            set_waker: Option<F>,
            slept: bool,
        }

        impl<F: FnOnce(&Waker)> Future for BlockTillWoken<F> {
            type Output = ();

            fn poll(
                self: core::pin::Pin<&mut Self>, cx: &mut Context<'_>,
            ) -> Poll<()> {
                if self.slept {
                    Poll::Ready(())
                } else {
                    let (set_waker, slept) = unsafe {
                        let me = self.get_unchecked_mut();
                        (me.set_waker.take().unwrap(), &mut me.slept)
                    };

                    set_waker(cx.waker());
                    *slept = true;
                    Poll::Pending
                }
            }
        }

        BlockTillWoken {
            set_waker: Some(set_waker),
            slept: false,
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

            println_trace!(
                feat: "trace_scheduler",
                "Switching to task {:?} (CPU{})",
                next.id,
                eonix_hal::processor::CPU::local().cpuid(),
            );

            let old_state = next.state.swap(TaskState::RUNNING);
            assert_eq!(
                old_state,
                TaskState::READY,
                "Next task should be in READY state"
            );

            let _old_current = Task::_swap_current(Some(next));
            debug_assert!(_old_current.is_none());
            unsafe {
                assert_unchecked(_old_current.is_none());
            }

            drop(rq);

            // TODO: MAYBE we can move the release of finished tasks to some worker thread.
            if Task::current().poll().is_ready() {
                let old_state = Task::current().state.swap(TaskState::DEAD);
                assert!(
                    old_state & TaskState::RUNNING != 0,
                    "Current task should be at least in RUNNING state"
                );

                println_trace!(
                    feat: "trace_scheduler",
                    "Task {:?} finished execution, removing...",
                    Task::current().id,
                );

                self.remove_task(&Task::current());

                Task::_swap_current(None);
            }
        }
    }
}
