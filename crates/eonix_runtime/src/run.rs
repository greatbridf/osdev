mod future_run;

use core::{pin::Pin, task::Waker};
pub use future_run::FutureRun;

pub enum RunState<Output> {
    Running,
    Finished(Output),
}

pub trait Contexted {
    /// # Safety
    /// This function should be called in a preemption disabled context.
    fn load_running_context(&self) {}

    /// # Safety
    /// This function should be called in a preemption disabled context.
    fn restore_running_context(&self) {}
}

pub trait Run {
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

pub trait PinRun {
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

impl<R> Run for R
where
    R: PinRun + Unpin,
{
    type Output = R::Output;

    fn run(&mut self, waker: &Waker) -> RunState<Self::Output> {
        Pin::new(self).pinned_run(waker)
    }
}
