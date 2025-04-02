use core::{pin::Pin, task::Waker};

pub enum RunState<Output> {
    Running,
    Finished(Output),
}

pub trait Contexted {
    /// # Safety
    /// This function will be called in a preemption disabled context.
    fn load_running_context(&mut self);

    /// # Safety
    /// This function will be called in a preemption disabled context.
    fn restore_running_context(&mut self);
}

pub trait Runnable {
    type Output;

    fn run(&mut self, waker: &Waker) -> RunState<Self::Output>;

    fn join(&mut self, waker: &Waker) -> Self::Output {
        loop {
            match self.run(waker) {
                RunState::Running => continue,
                RunState::Finished(output) => break output,
            }
        }
    }
}

pub trait PinRunnable {
    type Output;

    fn pinned_run(self: Pin<&mut Self>, waker: &Waker) -> RunState<Self::Output>;

    fn pinned_join(mut self: Pin<&mut Self>, waker: &Waker) -> Self::Output {
        loop {
            match self.as_mut().pinned_run(waker) {
                RunState::Running => continue,
                RunState::Finished(output) => break output,
            }
        }
    }
}

impl<R> Runnable for R
where
    R: PinRunnable + Unpin,
{
    type Output = R::Output;

    fn run(&mut self, waker: &Waker) -> RunState<Self::Output> {
        Pin::new(self).pinned_run(waker)
    }
}
