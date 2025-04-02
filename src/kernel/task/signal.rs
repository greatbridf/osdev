use core::{cmp::Reverse, task::Waker};

use crate::{
    io::BufferFill,
    kernel::{
        constants::{SA_RESTORER, SA_SIGINFO},
        user::{dataflow::UserBuffer, UserPointer},
    },
    prelude::*,
    sync::{preempt, AsRefPosition as _},
};

use alloc::collections::{binary_heap::BinaryHeap, btree_map::BTreeMap};
use arch::{ExtendedContext, InterruptContext};
use bindings::{EFAULT, EINVAL};

use super::{ProcessList, Scheduler, Task, Thread, WaitObject, WaitType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Signal(u32);

#[allow(dead_code)]
impl Signal {
    pub const SIGHUP: Signal = Signal(1);
    pub const SIGINT: Signal = Signal(2);
    pub const SIGQUIT: Signal = Signal(3);
    pub const SIGILL: Signal = Signal(4);
    pub const SIGTRAP: Signal = Signal(5);
    pub const SIGABRT: Signal = Signal(6);
    pub const SIGIOT: Signal = Signal(6);
    pub const SIGBUS: Signal = Signal(7);
    pub const SIGFPE: Signal = Signal(8);
    pub const SIGKILL: Signal = Signal(9);
    pub const SIGUSR1: Signal = Signal(10);
    pub const SIGSEGV: Signal = Signal(11);
    pub const SIGUSR2: Signal = Signal(12);
    pub const SIGPIPE: Signal = Signal(13);
    pub const SIGALRM: Signal = Signal(14);
    pub const SIGTERM: Signal = Signal(15);
    pub const SIGSTKFLT: Signal = Signal(16);
    pub const SIGCHLD: Signal = Signal(17);
    pub const SIGCONT: Signal = Signal(18);
    pub const SIGSTOP: Signal = Signal(19);
    pub const SIGTSTP: Signal = Signal(20);
    pub const SIGTTIN: Signal = Signal(21);
    pub const SIGTTOU: Signal = Signal(22);
    pub const SIGURG: Signal = Signal(23);
    pub const SIGXCPU: Signal = Signal(24);
    pub const SIGXFSZ: Signal = Signal(25);
    pub const SIGVTALRM: Signal = Signal(26);
    pub const SIGPROF: Signal = Signal(27);
    pub const SIGWINCH: Signal = Signal(28);
    pub const SIGIO: Signal = Signal(29);
    pub const SIGPOLL: Signal = Signal(29);
    pub const SIGPWR: Signal = Signal(30);
    pub const SIGSYS: Signal = Signal(31);
}

#[derive(Debug, Clone, Copy)]
pub struct SignalAction {
    pub sa_handler: usize,
    pub sa_flags: usize,
    pub sa_restorer: usize,
    pub sa_mask: usize,
}

#[derive(Debug)]
struct SignalListInner {
    mask: u64,
    pending: BinaryHeap<Reverse<Signal>>,

    signal_waker: Option<Waker>,
    stop_waker: Option<Waker>,

    // TODO!!!!!: Signal disposition should be per-process.
    handlers: BTreeMap<Signal, SignalAction>,
}

#[derive(Debug)]
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
                handlers: inner.handlers.clone(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RaiseResult {
    Finished,
    Masked,
}

impl Signal {
    const fn is_ignore(&self) -> bool {
        match *self {
            Self::SIGCHLD | Self::SIGURG | Self::SIGWINCH => true,
            _ => false,
        }
    }

    pub const fn is_now(&self) -> bool {
        match *self {
            Self::SIGKILL | Self::SIGSTOP => true,
            _ => false,
        }
    }

    pub const fn is_coredump(&self) -> bool {
        match *self {
            Self::SIGQUIT
            | Self::SIGILL
            | Self::SIGABRT
            | Self::SIGFPE
            | Self::SIGSEGV
            | Self::SIGBUS
            | Self::SIGTRAP
            | Self::SIGSYS
            | Self::SIGXCPU
            | Self::SIGXFSZ => true,
            _ => false,
        }
    }

    fn to_mask(&self) -> u64 {
        1 << (self.0 - 1)
    }
}

impl TryFrom<u32> for Signal {
    type Error = u32;

    fn try_from(signum: u32) -> Result<Self, Self::Error> {
        if signum > 0 && signum <= 64 {
            Ok(Self(signum))
        } else {
            Err(EINVAL)
        }
    }
}

impl From<Signal> for u32 {
    fn from(signal: Signal) -> Self {
        let Signal(signum) = signal;
        signum
    }
}

impl SignalAction {
    fn default_action() -> Self {
        Self {
            sa_handler: 0,
            sa_flags: 0,
            sa_restorer: 0,
            sa_mask: 0,
        }
    }

    fn is_ignore(&self) -> bool {
        const SIG_IGN: usize = 1;
        self.sa_handler == SIG_IGN
    }

    fn is_default(&self) -> bool {
        const SIG_DFL: usize = 0;
        self.sa_handler == SIG_DFL
    }

    /// # Might Sleep
    fn handle(
        &self,
        signal: Signal,
        old_mask: u64,
        int_stack: &mut InterruptContext,
        ext_ctx: &mut ExtendedContext,
    ) -> KResult<()> {
        if self.sa_flags & SA_RESTORER as usize == 0 {
            return Err(EINVAL);
        }

        const CONTEXT_SIZE: usize = size_of::<InterruptContext>()
            + size_of::<ExtendedContext>()
            + size_of::<usize>() // old_mask
            + size_of::<u32>(); // `sa_handler` argument: `signum`

        // Save current interrupt context to 128 bytes above current user stack
        // and align to 16 bytes. Then we push the return address of the restorer.
        // TODO!!!: Determine the size of the return address
        let sp = ((int_stack.rsp as usize - 128 - CONTEXT_SIZE) & !0xf) - size_of::<u32>();
        let restorer_address: u32 = self.sa_restorer as u32;
        let mut stack = UserBuffer::new(sp as *mut u8, CONTEXT_SIZE + size_of::<u32>())?;

        stack.copy(&restorer_address)?.ok_or(EFAULT)?; // Restorer address
        stack.copy(&u32::from(signal))?.ok_or(EFAULT)?; // Restorer address
        stack.copy(&old_mask)?.ok_or(EFAULT)?; // Original signal mask
        stack.copy(ext_ctx)?.ok_or(EFAULT)?; // MMX registers
        stack.copy(int_stack)?.ok_or(EFAULT)?; // Interrupt stack

        int_stack.rip = self.sa_handler as u64;
        int_stack.rsp = sp as u64;
        Ok(())
    }
}

impl SignalListInner {
    fn get_mask(&self) -> u64 {
        self.mask
    }

    fn set_mask(&mut self, mask: u64) {
        self.mask = mask;
    }

    fn mask(&mut self, mask: u64) {
        self.set_mask(self.mask | mask)
    }

    fn unmask(&mut self, mask: u64) {
        self.set_mask(self.mask & !mask)
    }

    fn is_masked(&self, signal: Signal) -> bool {
        self.mask & signal.to_mask() != 0
    }

    fn pop(&mut self) -> Option<Signal> {
        self.pending.pop().map(|Reverse(signal)| signal)
    }

    fn raise(&mut self, signal: Signal) -> RaiseResult {
        if self.is_masked(signal) {
            return RaiseResult::Masked;
        }

        match self.handlers.get(&signal) {
            // Ignore action
            Some(handler) if handler.is_ignore() => return RaiseResult::Finished,
            // Default action
            None if signal.is_ignore() => return RaiseResult::Finished,
            _ => {}
        }

        self.mask(signal.to_mask());
        self.pending.push(Reverse(signal));

        match signal {
            Signal::SIGCONT => {
                self.stop_waker.take().map(|waker| waker.wake());
            }
            _ => {
                // If we don't have a waker here, we might be at initialization step.
                // We would run in the end anyway.
                self.signal_waker
                    .as_ref()
                    .inspect(|waker| waker.wake_by_ref());
            }
        }

        return RaiseResult::Finished;
    }
}

impl SignalList {
    pub fn new() -> Self {
        Self {
            inner: Spin::new(SignalListInner {
                mask: 0,
                pending: BinaryHeap::new(),
                signal_waker: None,
                stop_waker: None,
                handlers: BTreeMap::new(),
            }),
        }
    }

    pub fn get_mask(&self) -> u64 {
        self.inner.lock().get_mask()
    }

    pub fn set_mask(&self, mask: u64) {
        self.inner.lock().set_mask(mask)
    }

    pub fn mask(&self, mask: u64) {
        self.inner.lock().set_mask(mask)
    }

    pub fn unmask(&self, mask: u64) {
        self.inner.lock().unmask(mask)
    }

    pub fn set_handler(&self, signal: Signal, action: &SignalAction) -> KResult<()> {
        if signal.is_now() || action.sa_flags & SA_SIGINFO as usize != 0 {
            return Err(EINVAL);
        }

        let mut inner = self.inner.lock();
        if action.is_default() {
            inner.handlers.remove(&signal);
        } else {
            inner.handlers.insert(signal, action.clone());
        }

        Ok(())
    }

    pub fn get_handler(&self, signal: Signal) -> SignalAction {
        self.inner
            .lock()
            .handlers
            .get(&signal)
            .cloned()
            .unwrap_or_else(SignalAction::default_action)
    }

    // TODO!!!: Find a better way.
    pub fn set_signal_waker(&self, waker: Waker) {
        let mut inner = self.inner.lock();
        let old_waker = inner.signal_waker.replace(waker);
        assert!(old_waker.is_none(), "We should not have a waker here");
    }

    /// Clear all signals except for `SIG_IGN`.
    /// This is used when `execve` is called.
    pub fn clear_non_ignore(&self) {
        self.inner
            .lock()
            .handlers
            .retain(|_, action| action.is_ignore());
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
    pub fn handle(&self, int_stack: &mut InterruptContext, ext_ctx: &mut ExtendedContext) {
        loop {
            let signal = {
                let signal = match self.inner.lock().pop() {
                    Some(signal) => signal,
                    None => return,
                };

                let handler = self.inner.lock().handlers.get(&signal).cloned();
                if let Some(handler) = handler {
                    if !signal.is_now() {
                        let old_mask = {
                            let mut inner = self.inner.lock();
                            let old_mask = inner.mask;
                            inner.mask(handler.sa_mask as u64);
                            old_mask
                        };
                        let result = handler.handle(signal, old_mask, int_stack, ext_ctx);
                        if result.is_err() {
                            self.inner.lock().set_mask(old_mask);
                        }
                        match result {
                            Err(EFAULT) => self.inner.lock().raise(Signal::SIGSEGV),
                            Err(_) => self.inner.lock().raise(Signal::SIGSYS),
                            Ok(()) => return,
                        };
                        continue;
                    }
                }

                // TODO: The default signal handling process should be atomic.

                // Default actions include stopping the thread, continuing the thread and
                // terminating the process. All these actions will block the thread or return
                // to the thread immediately. So we can unmask these signals now.
                self.inner.lock().unmask(signal.to_mask());
                signal
            };

            // Default actions.
            match signal {
                Signal::SIGSTOP | Signal::SIGTSTP | Signal::SIGTTIN | Signal::SIGTTOU => {
                    let thread = Thread::current();
                    if let Some(parent) = thread.process.parent.load() {
                        parent.notify(
                            WaitObject {
                                pid: thread.process.pid,
                                code: WaitType::Stopped(signal),
                            },
                            ProcessList::get().lock_shared().as_pos(),
                        );
                    }

                    preempt::disable();

                    // `SIGSTOP` can only be waken up by `SIGCONT` or `SIGKILL`.
                    // SAFETY: Preempt disabled above.
                    {
                        let mut inner = self.inner.lock();
                        let waker = Waker::from(Task::current().usleep());
                        let old_waker = inner.stop_waker.replace(waker);
                        assert!(old_waker.is_none(), "We should not have a waker here");
                    }

                    Scheduler::schedule();

                    if let Some(parent) = thread.process.parent.load() {
                        parent.notify(
                            WaitObject {
                                pid: thread.process.pid,
                                code: WaitType::Continued,
                            },
                            ProcessList::get().lock_shared().as_pos(),
                        );
                    }
                }
                Signal::SIGCONT => {}
                Signal::SIGKILL => ProcessList::kill_current(signal),
                // Ignored
                Signal::SIGCHLD | Signal::SIGURG | Signal::SIGWINCH => {}
                // TODO!!!!!!: Check exit status format.
                s if s.is_coredump() => ProcessList::kill_current(signal),
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

        self.inner.lock().set_mask(old_mask);
        Ok(int_stack.rax as usize)
    }
}
