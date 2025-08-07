mod signal_action;

use super::{ProcessList, Thread, WaitObject, WaitType};
use crate::kernel::constants::{EFAULT, EINVAL};
use crate::{kernel::user::UserPointer, prelude::*};
use alloc::collections::binary_heap::BinaryHeap;
use alloc::sync::Arc;
use core::{cmp::Reverse, task::Waker};
use eonix_hal::fpu::FpuState;
use eonix_hal::traits::trap::RawTrapContext;
use eonix_hal::trap::TrapContext;
use eonix_runtime::scheduler::Runtime;
use eonix_sync::AsProof as _;
use intrusive_collections::UnsafeRef;
use posix_types::signal::{SigSet, Signal};
use posix_types::{SIGNAL_IGNORE, SIGNAL_NOW, SIGNAL_STOP};
use signal_action::SignalActionList;

pub use signal_action::SignalAction;

pub(self) const SAVED_DATA_SIZE: usize =
    size_of::<TrapContext>() + size_of::<FpuState>() + size_of::<SigSet>();

struct SignalListInner {
    mask: SigSet,
    pending: BinaryHeap<Reverse<Signal>>,

    signal_waker: Option<UnsafeRef<dyn Fn() + Send + Sync>>,
    stop_waker: Option<Waker>,

    // TODO!!!!!: Signal disposition should be per-process.
    actions: Arc<SignalActionList>,
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
                self.mask.mask(SigSet::from(signal));
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
                mask: SigSet::empty(),
                pending: BinaryHeap::new(),
                signal_waker: None,
                stop_waker: None,
                actions: Arc::new(SignalActionList::new()),
            }),
        }
    }

    pub fn get_mask(&self) -> SigSet {
        self.inner.lock().mask
    }

    pub fn set_mask(&self, mask: SigSet) {
        self.inner.lock().mask = mask;
    }

    pub fn mask(&self, mask: SigSet) {
        self.inner.lock().mask.mask(mask)
    }

    pub fn unmask(&self, mask: SigSet) {
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
    pub async fn handle(&self, trap_ctx: &mut TrapContext, fpu_state: &mut FpuState) {
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

                    let result = handler.handle(signal, old_mask, trap_ctx, fpu_state);
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
                self.inner.lock().mask.unmask(SigSet::from(signal));
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
                            Some(Signal::SIGCHLD),
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
                    Runtime::block_till_woken(|waker| {
                        let mut inner = self.inner.lock();
                        let old_waker = inner.stop_waker.replace(waker.clone());
                        assert!(old_waker.is_none(), "We should not have a waker here");
                    })
                    .await;

                    if let Some(parent) = thread.process.parent.load() {
                        parent.notify(
                            Some(Signal::SIGCHLD),
                            WaitObject {
                                pid: thread.process.pid,
                                code: WaitType::Continued,
                            },
                            ProcessList::get().read().await.prove(),
                        );
                    }
                }
                signal => {
                    // Default to terminate the thread.
                    Thread::current().force_kill(signal).await;
                    return;
                }
            }
        }
    }

    /// Load the signal mask, fpu state and trap context from the user stack.
    pub fn restore(
        &self,
        trap_ctx: &mut TrapContext,
        fpu_state: &mut FpuState,
        old_sigreturn: bool,
    ) -> KResult<()> {
        #[cfg(not(any(
            target_arch = "x86_64",
            target_arch = "riscv64",
            target_arch = "loongarch64"
        )))]
        compile_error!("`restore` is not implemented for this architecture");

        #[cfg(target_arch = "x86_64")]
        let old_trap_ctx_vaddr = {
            let mut old_trap_ctx_vaddr = trap_ctx.get_stack_pointer() + 16;

            if old_sigreturn {
                // Old sigreturn will pop 4 bytes off the stack. We sub them back.

                use posix_types::ctypes::Long;
                old_trap_ctx_vaddr -= size_of::<Long>();
            }

            old_trap_ctx_vaddr
        };

        #[cfg(any(target_arch = "riscv64", target_arch = "loongarch64"))]
        let old_trap_ctx_vaddr = {
            debug_assert!(
                !old_sigreturn,
                "Old sigreturn is not supported on RISC-V and LoongArch64"
            );
            trap_ctx.get_stack_pointer()
        };

        let old_fpu_state_vaddr = old_trap_ctx_vaddr + size_of::<TrapContext>();
        let old_mask_vaddr = old_fpu_state_vaddr + size_of::<FpuState>();

        *trap_ctx = UserPointer::<TrapContext>::new_vaddr(old_trap_ctx_vaddr)?.read()?;

        // Make sure that at least we won't crash the kernel.
        if !trap_ctx.is_user_mode() || !trap_ctx.is_interrupt_enabled() {
            return Err(EFAULT)?;
        }

        *fpu_state = UserPointer::<FpuState>::new_vaddr(old_fpu_state_vaddr)?.read()?;
        self.inner.lock().mask = UserPointer::<SigSet>::new_vaddr(old_mask_vaddr)?.read()?;

        Ok(())
    }
}

impl SignalList {
    pub fn new_cloned(other: &Self) -> Self {
        let inner = other.inner.lock();

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
                actions: SignalActionList::new_cloned(&inner.actions),
            }),
        }
    }

    // shared only signal actions
    pub fn new_shared(other: &Self) -> Self {
        let inner = other.inner.lock();

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
                actions: SignalActionList::new_shared(&inner.actions),
            }),
        }
    }
}
