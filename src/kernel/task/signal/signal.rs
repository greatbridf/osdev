use crate::kernel::constants::EINVAL;

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

    pub const SIGNUM_MIN: u32 = 1;
    pub const SIGNUM_MAX: u32 = 64;
}

#[macro_export]
macro_rules! SIGNAL_IGNORE {
    () => {
        $crate::kernel::task::Signal::SIGCHLD
            | $crate::kernel::task::Signal::SIGURG
            | $crate::kernel::task::Signal::SIGWINCH
    };
}

#[macro_export]
macro_rules! SIGNAL_NOW {
    () => {
        $crate::kernel::task::Signal::SIGKILL | $crate::kernel::task::Signal::SIGSTOP
    };
}

#[macro_export]
macro_rules! SIGNAL_COREDUMP {
    () => {
        $crate::kernel::task::Signal::SIGQUIT
            | $crate::kernel::task::Signal::SIGILL
            | $crate::kernel::task::Signal::SIGABRT
            | $crate::kernel::task::Signal::SIGFPE
            | $crate::kernel::task::Signal::SIGSEGV
            | $crate::kernel::task::Signal::SIGBUS
            | $crate::kernel::task::Signal::SIGTRAP
            | $crate::kernel::task::Signal::SIGSYS
            | $crate::kernel::task::Signal::SIGXCPU
            | $crate::kernel::task::Signal::SIGXFSZ
    };
}

#[macro_export]
macro_rules! SIGNAL_STOP {
    () => {
        $crate::kernel::task::Signal::SIGSTOP
            | $crate::kernel::task::Signal::SIGTSTP
            | $crate::kernel::task::Signal::SIGTTIN
            | $crate::kernel::task::Signal::SIGTTOU
    };
}

impl TryFrom<u32> for Signal {
    type Error = u32;

    fn try_from(signum: u32) -> Result<Self, Self::Error> {
        match signum {
            Self::SIGNUM_MIN..=Self::SIGNUM_MAX => Ok(Self(signum)),
            _ => Err(EINVAL),
        }
    }
}

impl From<Signal> for u32 {
    fn from(signal: Signal) -> Self {
        let Signal(signum) = signal;
        signum
    }
}

impl From<Signal> for SignalMask {
    fn from(signal: Signal) -> Self {
        let signum = u32::from(signal);
        SignalMask::new(1 << (signum - 1))
    }
}

use super::SignalMask;

pub use {SIGNAL_IGNORE, SIGNAL_NOW, SIGNAL_STOP};
