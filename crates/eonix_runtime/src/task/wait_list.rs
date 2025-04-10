use super::task_state::TaskState;
use crate::task::Task;
use alloc::collections::vec_deque::VecDeque;
use core::{fmt, task::Waker};
use eonix_preempt::assert_preempt_enabled;
use eonix_sync::{sleep, Spin, UnlockableGuard, UnlockedGuard, WaitList};

pub struct TaskWait {
    waiters: Spin<VecDeque<Waker>>,
}

impl TaskWait {
    pub const fn new() -> Self {
        Self {
            waiters: Spin::new(VecDeque::new()),
        }
    }

    fn wake(waker: &Waker) {
        waker.wake_by_ref();
    }
}

impl WaitList for TaskWait {
    fn has_waiters(&self) -> bool {
        !self.waiters.lock().is_empty()
    }

    fn notify_one(&self) -> bool {
        self.waiters
            .lock()
            .pop_front()
            .inspect(Self::wake)
            .is_some()
    }

    fn notify_all(&self) -> usize {
        self.waiters.lock().drain(..).inspect(Self::wake).count()
    }

    fn wait<G>(&self, guard: G) -> impl Future<Output = G> + Send
    where
        Self: Sized,
        G: UnlockableGuard,
        G::Unlocked: Send,
    {
        let waker = Waker::from(Task::current().clone());
        self.waiters.lock().push_back(waker);

        Task::current().state.swap(TaskState::SLEEPING);

        let unlocked_guard = guard.unlock();
        assert_preempt_enabled!("TaskWait::wait()");

        async {
            sleep().await;

            unlocked_guard.relock()
        }
    }
}

impl fmt::Debug for TaskWait {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WaitList").finish()
    }
}
