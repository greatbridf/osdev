mod builder;
mod execute_status;
mod output_handle;
mod stack;

use crate::{
    run::{Contexted, Run, RunState},
    scheduler::Scheduler,
    task::Task,
};
use alloc::sync::Weak;
use core::{
    pin::Pin,
    sync::atomic::{compiler_fence, fence, AtomicBool, Ordering},
    task::Waker,
};
use eonix_sync::Spin;

pub use builder::ExecutorBuilder;
pub use execute_status::ExecuteStatus;
pub use output_handle::OutputHandle;
pub use stack::Stack;

/// An `Executor` executes a `Run` object in a separate thread of execution
/// where we have a dedicated stack and context.
pub trait Executor: Send {
    fn progress(&self) -> ExecuteStatus;
}

struct RealExecutor<S, R>
where
    R: Run + Send + Contexted + 'static,
    R::Output: Send,
{
    _stack: S,
    runnable: R,
    output_handle: Weak<Spin<OutputHandle<R::Output>>>,
    finished: AtomicBool,
}

impl<S, R> RealExecutor<S, R>
where
    R: Run + Send + Contexted + 'static,
    R::Output: Send,
{
    extern "C" fn execute(self: Pin<&Self>) -> ! {
        // We get here with preempt count == 1.
        eonix_preempt::enable();

        {
            let waker = Waker::from(Task::current().clone());

            let output_data = loop {
                // TODO!!!!!!: CHANGE THIS.
                let runnable_pointer = &raw const self.get_ref().runnable;

                // SAFETY: We don't move the runnable object and we MIGHT not be using the
                //         part that is used in `pinned_run` in the runnable...?
                let mut pinned_runnable =
                    unsafe { Pin::new_unchecked(&mut *(runnable_pointer as *mut R)) };

                match pinned_runnable.as_mut().run(&waker) {
                    RunState::Finished(output) => break output,
                    RunState::Running => Task::park(),
                }
            };

            if let Some(output_handle) = self.output_handle.upgrade() {
                output_handle.lock().commit_output(output_data);
            }
        }

        // SAFETY: We are on the same CPU as the task.
        self.finished.store(true, Ordering::Relaxed);

        unsafe {
            // SAFETY: `preempt::count()` == 1.
            eonix_preempt::disable();
            Scheduler::goto_scheduler_noreturn()
        }
    }
}

impl<S, R> Executor for RealExecutor<S, R>
where
    S: Send,
    R: Run + Contexted + Send,
    R::Output: Send,
{
    fn progress(&self) -> ExecuteStatus {
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
        compiler_fence(Ordering::SeqCst);

        // TODO!!!: We should load the context only if the previous task is
        // different from the current task.

        self.runnable.load_running_context();

        unsafe {
            // SAFETY: We are in the scheduler context and we are not preempted.
            Scheduler::go_from_scheduler(&Task::current().execution_context);
        }

        self.runnable.restore_running_context();

        compiler_fence(Ordering::SeqCst);
        fence(Ordering::SeqCst);

        if self.finished.load(Ordering::Acquire) {
            ExecuteStatus::Finished
        } else {
            ExecuteStatus::Executing
        }
    }
}
