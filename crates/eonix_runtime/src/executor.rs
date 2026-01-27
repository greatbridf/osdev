use alloc::boxed::Box;
use alloc::sync::{Arc, Weak};
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

use eonix_sync::Spin;

/// An `Executor` executes a Future object in a separate thread of execution.
///
/// When the Future is finished, the `Executor` will call the `OutputHandle` to
/// commit the output.  Then the `Executor` will release the resources
/// associated with the Future.
pub struct Executor(Option<Pin<Box<dyn TypeErasedExecutor>>>);

enum OutputState<Output>
where
    Output: Send,
{
    Waiting(Option<Waker>),
    Finished(Option<Output>),
    TakenOut,
}

pub struct OutputHandle<Output>
where
    Output: Send,
{
    inner: OutputState<Output>,
}

trait TypeErasedExecutor: Send {
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()>;
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
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.output_handle.as_ptr().is_null() {
            return Poll::Ready(());
        }

        let future = unsafe {
            // SAFETY: We don't move the future.
            self.as_mut().map_unchecked_mut(|me| &mut me.future)
        };

        future.poll(cx).map(|output| {
            if let Some(output_handle) = self.output_handle.upgrade() {
                output_handle.lock().commit_output(output);

                unsafe {
                    // SAFETY: `output_handle` is Unpin.
                    self.get_unchecked_mut().output_handle = Weak::new();
                }
            }
        })
    }
}

impl Executor {
    pub fn new<F>(future: F) -> (Self, Arc<Spin<OutputHandle<F::Output>>>)
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let output_handle = OutputHandle::new();

        (
            Executor(Some(Box::pin(RealExecutor {
                future,
                output_handle: Arc::downgrade(&output_handle),
                _phantom: PhantomData,
            }))),
            output_handle,
        )
    }

    pub fn poll(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        if let Some(executor) = self.0.as_mut() {
            executor.as_mut().poll(cx).map(|_| {
                self.0.take();
            })
        } else {
            Poll::Ready(())
        }
    }
}

impl<Output> OutputHandle<Output>
where
    Output: Send,
{
    pub fn new() -> Arc<Spin<Self>> {
        Arc::new(Spin::new(Self {
            inner: OutputState::Waiting(None),
        }))
    }

    pub fn try_resolve(&mut self) -> Option<Output> {
        let output = match &mut self.inner {
            OutputState::Waiting(_) => return None,
            OutputState::Finished(output) => output.take(),
            OutputState::TakenOut => panic!("Output already taken out"),
        };

        self.inner = OutputState::TakenOut;
        if let Some(output) = output {
            Some(output)
        } else {
            unreachable!("Output should be present")
        }
    }

    pub fn register_waiter(&mut self, waker: Waker) {
        if let OutputState::Waiting(inner_waker) = &mut self.inner {
            inner_waker.replace(waker);
        } else {
            panic!("Output is not waiting");
        }
    }

    pub fn commit_output(&mut self, output: Output) {
        if let OutputState::Waiting(inner_waker) = &mut self.inner {
            if let Some(waker) = inner_waker.take() {
                waker.wake();
            }
            self.inner = OutputState::Finished(Some(output));
        } else {
            panic!("Output is not waiting");
        }
    }
}
