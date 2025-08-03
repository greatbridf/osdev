use super::{ExecuteStatus, Executor, OutputHandle};
use crate::{
    context::ExecutionContext,
    run::{Contexted, Run, RunState},
    task::Task,
};

use eonix_sync::Spin;

use core::{
    pin::Pin,
    sync::atomic::AtomicBool,
    sync::atomic::{compiler_fence, fence, Ordering},
    task::Waker,
};

use alloc::{
    boxed::Box,
    sync::{Arc, Weak},
};

struct StacklessExecutor<R>
where
    R: Run + Send + Contexted + 'static,
    R::Output: Send,
{
    runnable: R,
    output_handle: Weak<Spin<OutputHandle<R::Output>>>,
    finished: AtomicBool,
}

impl<R> Executor for StacklessExecutor<R>
where
    R: Run + Contexted + Send,
    R::Output: Send,
{
    fn progress(&self) -> ExecuteStatus {
        fence(Ordering::SeqCst);
        compiler_fence(Ordering::SeqCst);

        // TODO!!!: We should load the context only if the previous task is
        // different from the current task.

        self.runnable.load_running_context();

        let waker = Waker::from(Task::current().clone());

        let runnable_pointer = &raw const self.runnable;

        // SAFETY: We don't move the runnable object and we MIGHT not be using the
        //         part that is used in `pinned_run` in the runnable...?
        let mut pinned_runnable = unsafe { Pin::new_unchecked(&mut *(runnable_pointer as *mut R)) };

        // Satisfy some preempt check
        eonix_preempt::enable();
        match pinned_runnable.as_mut().run(&waker) {
            RunState::Finished(output) => {
                if let Some(output_handle) = self.output_handle.upgrade() {
                    output_handle.lock().commit_output(output);
                }

                self.finished.store(true, Ordering::Relaxed);
            }
            RunState::Running => {}
        }
        eonix_preempt::disable();

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

pub struct StacklessExecutorBuilder<R> {
    runnable: Option<R>,
}

impl<R> StacklessExecutorBuilder<R>
where
    R: Run + Contexted + Send + 'static,
    R::Output: Send,
{
    pub fn new() -> Self {
        Self { runnable: None }
    }

    pub fn runnable(mut self, runnable: R) -> Self {
        self.runnable.replace(runnable);
        self
    }

    pub fn build(
        mut self,
    ) -> (
        Pin<Box<dyn Executor>>,
        Option<ExecutionContext>,
        Arc<Spin<OutputHandle<R::Output>>>,
    ) {
        let runnable = self.runnable.take().expect("Runnable is required");

        let output_handle = OutputHandle::new();

        let executor = Box::pin(StacklessExecutor {
            runnable,
            output_handle: Arc::downgrade(&output_handle),
            finished: AtomicBool::new(false),
        });

        (executor, None, output_handle)
    }
}
