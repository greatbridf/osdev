use super::{Executor, OutputHandle, RealExecutor, Stack};
use crate::{
    context::ExecutionContext,
    run::{Contexted, Run},
};
use alloc::{boxed::Box, sync::Arc};
use core::{pin::Pin, sync::atomic::AtomicBool};
use eonix_sync::Spin;

pub struct ExecutorBuilder<S, R> {
    stack: Option<S>,
    runnable: Option<R>,
}

impl<S, R> ExecutorBuilder<S, R>
where
    S: Stack,
    R: Run + Contexted + Send + 'static,
    R::Output: Send,
{
    pub fn new() -> Self {
        Self {
            stack: None,
            runnable: None,
        }
    }

    pub fn stack(mut self, stack: S) -> Self {
        self.stack.replace(stack);
        self
    }

    pub fn runnable(mut self, runnable: R) -> Self {
        self.runnable.replace(runnable);
        self
    }

    pub fn build(
        mut self,
    ) -> (
        Pin<Box<impl Executor>>,
        ExecutionContext,
        Arc<Spin<OutputHandle<R::Output>>>,
    ) {
        let stack = self.stack.take().expect("Stack is required");
        let runnable = self.runnable.take().expect("Runnable is required");

        let mut execution_context = ExecutionContext::new();
        let output_handle = OutputHandle::new();

        execution_context.set_sp(stack.get_bottom() as *const _ as _);

        let executor = Box::pin(RealExecutor {
            _stack: stack,
            runnable,
            output_handle: Arc::downgrade(&output_handle),
            finished: AtomicBool::new(false),
        });

        execution_context.call1(
            RealExecutor::<S, R>::execute,
            executor.as_ref().get_ref() as *const _ as usize,
        );

        (executor, execution_context, output_handle)
    }
}
