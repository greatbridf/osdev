use super::{Contexted, Run, RunState};
use core::{
    pin::Pin,
    task::{Context, Poll, Waker},
};

pub struct FutureRun<F: Future>(F);

impl<F> FutureRun<F>
where
    F: Future,
{
    pub const fn new(future: F) -> Self {
        Self(future)
    }
}

impl<F> Contexted for FutureRun<F> where F: Future {}
impl<F> Run for FutureRun<F>
where
    F: Future + 'static,
{
    type Output = F::Output;

    fn run(self: Pin<&mut Self>, waker: &Waker) -> RunState<Self::Output> {
        let mut future = unsafe { self.map_unchecked_mut(|me| &mut me.0) };
        let mut context = Context::from_waker(waker);

        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => RunState::Finished(output),
            Poll::Pending => RunState::Running,
        }
    }
}
