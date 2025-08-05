// mod builder;
mod output_handle;
mod stack;

use alloc::{
    boxed::Box,
    sync::{Arc, Weak},
};
use core::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use eonix_sync::Spin;

pub use output_handle::OutputHandle;
pub use stack::Stack;

/// An `Executor` executes a Future object in a separate thread of execution.
///
/// When the Future is finished, the `Executor` will call the `OutputHandle` to commit the output.
/// Then the `Executor` will release the resources associated with the Future.
pub struct Executor(Option<Pin<Box<dyn TypeErasedExecutor>>>);

trait TypeErasedExecutor: Send {
    /// # Returns
    /// Whether the executor has finished.
    fn run(self: Pin<&mut Self>, cx: &mut Context<'_>) -> bool;
}

struct RealExecutor<'a, F>
where
    F: Future + Send + 'a,
    F::Output: Send + 'a,
{
    future: F,
    output_handle: Weak<Spin<OutputHandle<F::Output>>>,
    _phantom: PhantomData<&'a ()>,
}

impl<F> TypeErasedExecutor for RealExecutor<'_, F>
where
    F: Future + Send,
    F::Output: Send,
{
    fn run(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> bool {
        if self.output_handle.as_ptr().is_null() {
            return true;
        }

        let future = unsafe {
            // SAFETY: We don't move the future.
            self.as_mut().map_unchecked_mut(|me| &mut me.future)
        };

        match future.poll(cx) {
            Poll::Ready(output) => {
                if let Some(output_handle) = self.output_handle.upgrade() {
                    output_handle.lock().commit_output(output);

                    unsafe {
                        // SAFETY: `output_handle` is Unpin.
                        self.get_unchecked_mut().output_handle = Weak::new();
                    }
                }

                true
            }
            Poll::Pending => false,
        }
    }
}

impl Executor {
    pub fn new<F>(future: F) -> (Self, Arc<Spin<OutputHandle<F::Output>>>)
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let output_handle = OutputHandle::new();

        // TODO: accept futures with non 'static lifetimes.
        (
            Executor(Some(Box::pin(RealExecutor {
                future,
                output_handle: Arc::downgrade(&output_handle),
                _phantom: PhantomData,
            }))),
            output_handle,
        )
    }

    pub fn run(&mut self, cx: &mut Context<'_>) -> bool {
        if let Some(executor) = self.0.as_mut() {
            let finished = executor.as_mut().run(cx);
            if finished {
                self.0.take();
            }

            finished
        } else {
            true
        }
    }
}
