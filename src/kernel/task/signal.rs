use core::cmp::Reverse;

use crate::{io::BufferFill, kernel::user::dataflow::UserBuffer, prelude::*};

use alloc::collections::{binary_heap::BinaryHeap, btree_map::BTreeMap};
use bindings::{
    interrupt_stack, kill_current, mmx_registers, EFAULT, EINVAL, SA_RESTORER, SIGABRT, SIGBUS,
    SIGCHLD, SIGCONT, SIGFPE, SIGILL, SIGKILL, SIGQUIT, SIGSEGV, SIGSTOP, SIGSYS, SIGTRAP, SIGTSTP,
    SIGTTIN, SIGTTOU, SIGURG, SIGWINCH, SIGXCPU, SIGXFSZ,
};

use super::Thread;

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
    inner: Mutex<SignalListInner>,
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

    fn is_coredump(&self) -> bool {
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

    /// # Return
    /// `(new_ip, new_sp)`
    ///
    /// # Might Sleep
    fn handle(
        &self,
        signum: u32,
        int_stack: &mut interrupt_stack,
        mmxregs: &mut mmx_registers,
    ) -> KResult<(usize, usize)> {
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

        Ok((self.sa_handler, sp))
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
            inner: Mutex::new(SignalListInner {
                mask: 0,
                pending: BinaryHeap::new(),
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
        if signal.is_now() {
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

    /// # Safety
    /// This function might never return. Caller must make sure that local variables
    /// that own resources are dropped before calling this function.
    ///
    /// # Return
    /// `(new_ip, new_sp)`
    pub fn handle(
        &self,
        int_stack: &mut interrupt_stack,
        mmxregs: &mut mmx_registers,
    ) -> Option<(usize, usize)> {
        let mut inner = self.inner.lock();

        loop {
            let signal = match self.inner.lock().pop() {
                Some(signal) => signal,
                None => return None,
            };

            if signal.is_now() {
                match signal {
                    Signal::SIGKILL => terminate_process(signal),
                    Signal::SIGSTOP => {
                        Thread::current().do_stop();
                        inner.unmask(signal.to_mask());
                    }
                    _ => unreachable!(),
                }
            }

            match inner.handlers.get(&signal) {
                // Default action
                None => {
                    match signal {
                        s if s.is_continue() => {
                            Thread::current().do_continue();
                            inner.unmask(signal.to_mask());
                            return None;
                        }
                        s if s.is_stop() => {
                            Thread::current().do_stop();
                            inner.unmask(signal.to_mask());
                            continue;
                        }
                        s if s.is_coredump() => terminate_process_core_dump(signal),
                        s if !s.is_ignore() => terminate_process(signal),
                        _ => continue, // Ignore
                    }
                }
                Some(handler) => {
                    let result = handler.handle(signal.to_signum(), int_stack, mmxregs);
                    match result {
                        Err(EFAULT) => inner.raise(Signal::SIGSEGV),
                        Err(_) => inner.raise(Signal::SIGSYS),
                        Ok((ip, sp)) => return Some((ip, sp)),
                    };
                    continue;
                }
            }
        }
    }
}

// TODO!!!: Should we use `uwake` or `iwake`?
fn terminate_process(signal: Signal) -> ! {
    unsafe { kill_current(signal.to_signum() as i32) };
}

fn terminate_process_core_dump(signal: Signal) -> ! {
    unsafe { kill_current(signal.to_signum() as i32 & 0x80) };
}

fn schedule() {}
