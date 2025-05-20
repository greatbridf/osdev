mod signal;
mod signal_action;
mod signal_mask;

use super::{ProcessList, Thread, WaitObject, WaitType};
use crate::{kernel::user::UserPointer, prelude::*};
use alloc::collections::binary_heap::BinaryHeap;
use arch::{ExtendedContext, InterruptContext};
use bindings::{EFAULT, EINVAL};
use core::{cmp::Reverse, task::Waker};
use eonix_runtime::task::Task;
use eonix_sync::AsProof as _;
use intrusive_collections::UnsafeRef;
use signal_action::SignalActionList;

pub use signal::{Signal, SIGNAL_IGNORE, SIGNAL_NOW, SIGNAL_STOP};
pub use signal_action::SignalAction;
pub use signal_mask::SignalMask;

struct SignalListInner {
    mask: SignalMask,
    pending: BinaryHeap<Reverse<Signal>>,

    signal_waker: Option<UnsafeRef<dyn Fn() + Send + Sync>>,
    stop_waker: Option<Waker>,

    // TODO!!!!!: Signal disposition should be per-process.
    actions: SignalActionList,
}

pub struct SignalList {
    inner: Spin<SignalListInner>,
}

impl Clone for SignalList {
    fn clone(&self) -> Self {
        let inner = self.inner.lock();

        debug_assert!(
            inner.stop_waker.is_none(),
            "We should not have a stop waker here"
        );

        Self {
            inner: Spin::new(SignalListInner {
                mask: inner.mask,
                pending: BinaryHeap::new(),
                signal_waker: None,
                stop_waker: None,
                actions: inner.actions.clone(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RaiseResult {
    Finished,
    Masked,
}

impl SignalListInner {
    fn pop(&mut self) -> Option<Signal> {
        self.pending.pop().map(|Reverse(signal)| signal)
    }

    fn raise(&mut self, signal: Signal) -> RaiseResult {
        if self.mask.include(signal) {
            return RaiseResult::Masked;
        }

        match (signal, self.actions.get(signal)) {
            (_, SignalAction::Ignore) => {}
            (SIGNAL_IGNORE!(), SignalAction::Default) => {}
            _ => {
                self.mask.mask(SignalMask::from(signal));
                self.pending.push(Reverse(signal));

                if matches!(signal, Signal::SIGCONT) {
                    self.stop_waker.take().map(|waker| waker.wake());
                } else {
                    // If we don't have a waker here, we are not permitted to be woken up.
                    // We would run in the end anyway.
                    if let Some(waker) = self.signal_waker.take() {
                        waker();
                    }
                }
            }
        }

        RaiseResult::Finished
    }
}

impl SignalList {
    pub fn new() -> Self {
        Self {
            inner: Spin::new(SignalListInner {
                mask: SignalMask::empty(),
                pending: BinaryHeap::new(),
                signal_waker: None,
                stop_waker: None,
                actions: SignalActionList::new(),
            }),
        }
    }

    pub fn get_mask(&self) -> SignalMask {
        self.inner.lock().mask
    }

    pub fn set_mask(&self, mask: SignalMask) {
        self.inner.lock().mask = mask;
    }

    pub fn mask(&self, mask: SignalMask) {
        self.inner.lock().mask.mask(mask)
    }

    pub fn unmask(&self, mask: SignalMask) {
        self.inner.lock().mask.unmask(mask)
    }

    pub fn set_action(&self, signal: Signal, action: SignalAction) -> KResult<()> {
        if matches!(signal, SIGNAL_NOW!()) {
            return Err(EINVAL);
        }

        self.inner.lock().actions.set(signal, action);
        Ok(())
    }

    pub fn get_action(&self, signal: Signal) -> SignalAction {
        self.inner.lock().actions.get(signal)
    }

    pub fn set_signal_waker(&self, waker: Option<UnsafeRef<dyn Fn() + Send + Sync>>) {
        let mut inner = self.inner.lock();
        inner.signal_waker = waker;
    }

    /// Clear all signals except for `SIG_IGN`.
    /// This is used when `execve` is called.
    pub fn clear_non_ignore(&self) {
        self.inner.lock().actions.remove_non_ignore();
    }

    /// Clear all pending signals.
    /// This is used when `fork` is called.
    pub fn clear_pending(&self) {
        self.inner.lock().pending.clear()
    }

    pub fn has_pending_signal(&self) -> bool {
        !self.inner.lock().pending.is_empty()
    }

    /// Do not use this, use `Thread::raise` instead.
    pub(super) fn raise(&self, signal: Signal) -> RaiseResult {
        self.inner.lock().raise(signal)
    }

    /// Handle signals in the context of `Thread::current()`.
    ///
    /// # Safety
    /// This function might never return. Caller must make sure that local variables
    /// that own resources are dropped before calling this function.
    pub async fn handle(&self, int_stack: &mut InterruptContext, ext_ctx: &mut ExtendedContext) {
        loop {
            let signal = {
                let signal = match self.inner.lock().pop() {
                    Some(signal) => signal,
                    None => return,
                };

                let handler = self.inner.lock().actions.get(signal);
                if let SignalAction::SimpleHandler { mask, .. } = &handler {
                    let old_mask = {
                        let mut inner = self.inner.lock();
                        let old_mask = inner.mask;
                        inner.mask.mask(*mask);
                        old_mask
                    };

                    let result = handler.handle(signal, old_mask, int_stack, ext_ctx);
                    if result.is_err() {
                        self.inner.lock().mask = old_mask;
                    }
                    match result {
                        Err(EFAULT) => self.inner.lock().raise(Signal::SIGSEGV),
                        Err(_) => self.inner.lock().raise(Signal::SIGSYS),
                        Ok(()) => return,
                    };
                    continue;
                }

                // TODO: The default signal handling process should be atomic.

                // Default actions include stopping the thread, continuing the thread and
                // terminating the process. All these actions will block the thread or return
                // to the thread immediately. So we can unmask these signals now.
                self.inner.lock().mask.unmask(SignalMask::from(signal));
                signal
            };

            // Default actions.
            match signal {
                SIGNAL_IGNORE!() => {}
                Signal::SIGCONT => {
                    // SIGCONT wakeup is done in `raise()`. So no further action needed here.
                }
                SIGNAL_STOP!() => {
                    let thread = Thread::current();
                    if let Some(parent) = thread.process.parent.load() {
                        parent.notify(
                            WaitObject {
                                pid: thread.process.pid,
                                code: WaitType::Stopped(signal),
                            },
                            ProcessList::get().read().await.prove(),
                        );
                    }

                    eonix_preempt::disable();

                    // `SIGSTOP` can only be waken up by `SIGCONT` or `SIGKILL`.
                    // SAFETY: Preempt disabled above.
                    {
                        let mut inner = self.inner.lock();
                        let waker = Waker::from(Task::current().clone());

                        let old_waker = inner.stop_waker.replace(waker);
                        assert!(old_waker.is_none(), "We should not have a waker here");
                    }

                    Task::park_preempt_disabled();

                    if let Some(parent) = thread.process.parent.load() {
                        parent.notify(
                            WaitObject {
                                pid: thread.process.pid,
                                code: WaitType::Continued,
                            },
                            ProcessList::get().read().await.prove(),
                        );
                    }
                }
                // Default to terminate the process.
                signal => ProcessList::kill_current(signal),
            }
        }
    }

    /// Load the signal mask, MMX registers and interrupt stack from the user stack.
    /// We must be here because `sigreturn` is called. Se we return the value of the register
    /// used to store the syscall return value to prevent the original value being clobbered.
    pub fn restore(
        &self,
        int_stack: &mut InterruptContext,
        ext_ctx: &mut ExtendedContext,
    ) -> KResult<usize> {
        let old_mask_vaddr = int_stack.rsp as usize;
        let old_mmxregs_vaddr = old_mask_vaddr + size_of::<usize>();
        let old_int_stack_vaddr = old_mmxregs_vaddr + size_of::<ExtendedContext>();

        let old_mask = UserPointer::<u64>::new_vaddr(old_mask_vaddr)?.read()?;
        *ext_ctx = UserPointer::<ExtendedContext>::new_vaddr(old_mmxregs_vaddr)?.read()?;
        *int_stack = UserPointer::<InterruptContext>::new_vaddr(old_int_stack_vaddr)?.read()?;

        self.inner.lock().mask = SignalMask::from(old_mask);
        Ok(int_stack.rax as usize)
    }
}
