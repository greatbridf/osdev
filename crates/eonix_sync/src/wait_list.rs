use crate::UnlockableGuard;
use core::{future::poll_fn, task::Poll};

pub trait WaitList {
    fn has_waiters(&self) -> bool;
    fn notify_one(&self) -> bool;
    fn notify_all(&self) -> usize;

    fn wait<G>(&self, guard: G) -> impl Future<Output = G> + Send
    where
        Self: Sized,
        G: UnlockableGuard,
        G::Unlocked: Send;
}

pub async fn yield_now() {
    let mut yielded = false;
    poll_fn(|ctx| {
        if yielded {
            Poll::Ready(())
        } else {
            yielded = true;
            ctx.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await;
}

pub async fn sleep() {
    let mut sleeped = false;
    poll_fn(|_| {
        if sleeped {
            Poll::Ready(())
        } else {
            sleeped = true;
            Poll::Pending
        }
    })
    .await;
}
