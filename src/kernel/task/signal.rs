use core::cmp::Reverse;

use crate::{io::BufferFill, kernel::user::dataflow::UserBuffer, prelude::*};

use alloc::collections::{binary_heap::BinaryHeap, btree_map::BTreeMap};
use bindings::{
    interrupt_stack, mmx_registers, EFAULT, EINVAL, SA_RESTORER, SIGABRT, SIGBUS, SIGCHLD, SIGCONT,
    SIGFPE, SIGILL, SIGKILL, SIGQUIT, SIGSEGV, SIGSTOP, SIGSYS, SIGTRAP, SIGTSTP, SIGTTIN, SIGTTOU,
    SIGURG, SIGWINCH, SIGXCPU, SIGXFSZ,
};

use super::{ProcessList, Thread};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Signal(u32);

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
    sa_handler: usize,
    sa_flags: usize,
    sa_restorer: usize,
    sa_mask: usize,
}

#[derive(Debug, Clone)]
struct SignalListInner {
    mask: u64,
    pending: BinaryHeap<Reverse<Signal>>,

    // TODO!!!!!: Signal disposition should be per-process.
    handlers: BTreeMap<Signal, SignalAction>,
}

#[derive(Debug, Clone)]
pub struct SignalList {
    /// We might use this inside interrupt handler, so we need to use `lock_irq`.
    inner: Spin<SignalListInner>,
}

#[derive(Debug, Clone, Copy)]
pub enum RaiseResult {
    ShouldIWakeUp,
    ShouldUWakeUp,
    Finished,
    Masked,
}

impl Signal {
    fn is_continue(&self) -> bool {
        self.0 == SIGCONT
    }

    fn is_stop(&self) -> bool {
        match self.0 {
            SIGSTOP | SIGTSTP | SIGTTIN | SIGTTOU => true,
            _ => false,
        }
    }

    fn is_ignore(&self) -> bool {
        match self.0 {
            SIGCHLD | SIGURG | SIGWINCH => true,
            _ => false,
        }
    }

    pub fn is_now(&self) -> bool {
        match self.0 {
            SIGKILL | SIGSTOP => true,
            _ => false,
        }
    }

    pub fn is_coredump(&self) -> bool {
        match self.0 {
            SIGQUIT | SIGILL | SIGABRT | SIGFPE | SIGSEGV | SIGBUS | SIGTRAP | SIGSYS | SIGXCPU
            | SIGXFSZ => true,
            _ => false,
        }
    }

    fn to_mask(&self) -> u64 {
        1 << (self.0 - 1)
    }

    pub fn to_signum(&self) -> u32 {
        self.0
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
        signum: u32,
        int_stack: &mut interrupt_stack,
        mmxregs: &mut mmx_registers,
    ) -> KResult<()> {
        if self.sa_flags & SA_RESTORER as usize == 0 {
            return Err(EINVAL);
        }

        const CONTEXT_SIZE: usize = size_of::<interrupt_stack>()
            + size_of::<mmx_registers>()
            + 2 * size_of::<u32>() // Signum and address of sa_restorer
            + size_of::<usize>(); // Original RSP

        // Save current interrupt context to 128 bytes above current user stack
        // and align to 16 bytes
        // TODO!!!: Determine the size of the return address
        let sp = (int_stack.rsp - (128 + CONTEXT_SIZE + size_of::<u32>())) & !0xf;
        let restorer_address: u32 = self.sa_restorer as u32;
        let mut stack = UserBuffer::new(sp as *mut _, CONTEXT_SIZE)?;

        stack.copy(&restorer_address)?.ok_or(EFAULT)?; // Restorer address
        stack.copy(&signum)?.ok_or(EFAULT)?; // Signal number
        stack.copy(&int_stack.rsp)?.ok_or(EFAULT)?; // Original RSP
        stack.copy(mmxregs)?.ok_or(EFAULT)?; // MMX registers
        stack.copy(int_stack)?.ok_or(EFAULT)?; // Interrupt stack

        int_stack.v_rip = self.sa_handler;
        int_stack.rsp = sp;
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

        if signal.is_stop() {
            return RaiseResult::Finished;
        }

        // TODO!!!!!!: Fix this. SIGCONT could wake up USleep threads.
        if signal.is_continue() {
            return RaiseResult::ShouldUWakeUp;
        }

        return RaiseResult::ShouldIWakeUp;
    }
}

impl SignalList {
    pub fn new() -> Self {
        Self {
            inner: Spin::new(SignalListInner {
                mask: 0,
                pending: BinaryHeap::new(),
                handlers: BTreeMap::new(),
            }),
        }
    }

    pub fn get_mask(&self) -> u64 {
        self.inner.lock_irq().get_mask()
    }

    pub fn set_mask(&self, mask: u64) {
        self.inner.lock_irq().set_mask(mask)
    }

    pub fn mask(&self, mask: u64) {
        self.inner.lock_irq().set_mask(mask)
    }

    pub fn unmask(&self, mask: u64) {
        self.inner.lock_irq().unmask(mask)
    }

    pub fn set_handler(&self, signal: Signal, action: &SignalAction) -> KResult<()> {
        if signal.is_now() {
            return Err(EINVAL);
        }

        let mut inner = self.inner.lock_irq();
        if action.is_default() {
            inner.handlers.remove(&signal);
        } else {
            inner.handlers.insert(signal, action.clone());
        }

        Ok(())
    }

    pub fn get_handler(&self, signal: Signal) -> SignalAction {
        self.inner
            .lock_irq()
            .handlers
            .get(&signal)
            .cloned()
            .unwrap_or_else(SignalAction::default_action)
    }

    /// Clear all signals except for `SIG_IGN`.
    /// This is used when `execve` is called.
    pub fn clear_non_ignore(&self) {
        self.inner
            .lock_irq()
            .handlers
            .retain(|_, action| action.is_ignore());
    }

    /// Clear all pending signals.
    /// This is used when `fork` is called.
    pub fn clear_pending(&self) {
        self.inner.lock_irq().pending.clear()
    }

    pub fn has_pending_signal(&self) -> bool {
        !self.inner.lock_irq().pending.is_empty()
    }

    /// Do not use this, use `Thread::raise` instead.
    pub(super) fn raise(&self, signal: Signal) -> RaiseResult {
        self.inner.lock_irq().raise(signal)
    }

    /// Handle signals in the context of `Thread::current()`.
    ///
    /// # Safety
    /// This function might never return. Caller must make sure that local variables
    /// that own resources are dropped before calling this function.
    pub fn handle(&self, int_stack: &mut interrupt_stack, mmxregs: &mut mmx_registers) {
        loop {
            let signal = {
                let mut inner = self.inner.lock_irq();
                let signal = match inner.pop() {
                    Some(signal) => signal,
                    None => return,
                };

                if let Some(handler) = inner.handlers.get(&signal) {
                    if !signal.is_now() {
                        let result = handler.handle(signal.to_signum(), int_stack, mmxregs);
                        match result {
                            Err(EFAULT) => inner.raise(Signal::SIGSEGV),
                            Err(_) => inner.raise(Signal::SIGSYS),
                            Ok(()) => return,
                        };
                        continue;
                    }
                }

                // Default actions include stopping the thread, continuing the thread and
                // terminating the process. All these actions will block the thread or return
                // to the thread immediately. So we can unmask these signals now.
                inner.unmask(signal.to_mask());
                signal
            };

            // Default actions.
            match signal {
                Signal::SIGSTOP => Thread::current().do_stop(Signal::SIGSTOP),
                Signal::SIGCONT => Thread::current().do_continue(),
                Signal::SIGKILL => ProcessList::kill_current(signal),
                // Ignored
                Signal::SIGCHLD | Signal::SIGURG | Signal::SIGWINCH => continue,
                // "Soft" stops.
                Signal::SIGTSTP | Signal::SIGTTIN | Signal::SIGTTOU => {
                    Thread::current().do_stop(signal)
                }
                // TODO!!!!!!: Check exit status format.
                s if s.is_coredump() => ProcessList::kill_current(signal),
                signal => ProcessList::kill_current(signal),
            }
        }
    }
}
