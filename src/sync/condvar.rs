use crate::kernel::task::Thread;
use core::pin::pin;
use eonix_sync::{UnlockableGuard, UnlockedGuard as _, WaitList};
use intrusive_collections::UnsafeRef;

pub struct CondVar<const INTERRUPTIBLE: bool> {
    wait_list: WaitList,
}

impl<const I: bool> core::fmt::Debug for CondVar<I> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if I {
            f.debug_struct("CondVar").finish()
        } else {
            f.debug_struct("CondVarUnintrruptible").finish()
        }
    }
}

impl<const I: bool> CondVar<I> {
    pub const fn new() -> Self {
        Self {
            wait_list: WaitList::new(),
        }
    }

    pub fn notify_all(&self) {
        self.wait_list.notify_all();
    }

    /// Unlock the `guard`. Then wait until being waken up.
    /// Return the relocked `guard`.
    pub async fn wait<G>(&self, guard: G) -> G
    where
        G: UnlockableGuard + Send,
        G::Unlocked: Send,
    {
        let mut wait_handle = pin!(self.wait_list.prepare_to_wait());
        wait_handle.as_mut().add_to_wait_list();

        let interrupt_waker = pin!(|| {});

        if I {
            // Prohibit the thread from being woken up by a signal.
            Thread::current().signal_list.set_signal_waker(Some(unsafe {
                UnsafeRef::from_raw(interrupt_waker.as_ref().get_ref())
            }));
        }

        let unlocked_guard = guard.unlock();

        wait_handle.await;

        if I {
            // Allow the thread to be woken up by a signal again.
            Thread::current().signal_list.set_signal_waker(None);
        }

        unlocked_guard.relock().await
    }
}
