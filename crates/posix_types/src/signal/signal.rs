use crate::result::PosixError;
use core::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

    pub fn into_raw(self) -> u32 {
        self.0
    }

    pub fn try_from_raw(signo: u32) -> Result<Self, PosixError> {
        match signo {
            ..Self::SIGNUM_MAX => Ok(Signal(signo)),
            _ => Err(PosixError::EINVAL),
        }
    }
}

impl fmt::Debug for Signal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            &Signal(0) => write!(f, "Signal::EMPTY"),
            &Signal::SIGHUP => write!(f, "SIGHUP"),
            &Signal::SIGINT => write!(f, "SIGINT"),
            &Signal::SIGQUIT => write!(f, "SIGQUIT"),
            &Signal::SIGILL => write!(f, "SIGILL"),
            &Signal::SIGTRAP => write!(f, "SIGTRAP"),
            &Signal::SIGABRT => write!(f, "SIGABRT"),
            &Signal::SIGBUS => write!(f, "SIGBUS"),
            &Signal::SIGFPE => write!(f, "SIGFPE"),
            &Signal::SIGKILL => write!(f, "SIGKILL"),
            &Signal::SIGUSR1 => write!(f, "SIGUSR1"),
            &Signal::SIGSEGV => write!(f, "SIGSEGV"),
            &Signal::SIGUSR2 => write!(f, "SIGUSR2"),
            &Signal::SIGPIPE => write!(f, "SIGPIPE"),
            &Signal::SIGALRM => write!(f, "SIGALRM"),
            &Signal::SIGTERM => write!(f, "SIGTERM"),
            &Signal::SIGSTKFLT => write!(f, "SIGSTKFLT"),
            &Signal::SIGCHLD => write!(f, "SIGCHLD"),
            &Signal::SIGCONT => write!(f, "SIGCONT"),
            &Signal::SIGSTOP => write!(f, "SIGSTOP"),
            &Signal::SIGTSTP => write!(f, "SIGTSTP"),
            &Signal::SIGTTIN => write!(f, "SIGTTIN"),
            &Signal::SIGTTOU => write!(f, "SIGTTOU"),
            &Signal::SIGURG => write!(f, "SIGURG"),
            &Signal::SIGXCPU => write!(f, "SIGXCPU"),
            &Signal::SIGXFSZ => write!(f, "SIGXFSZ"),
            &Signal::SIGVTALRM => write!(f, "SIGVTALRM"),
            &Signal::SIGPROF => write!(f, "SIGPROF"),
            &Signal::SIGWINCH => write!(f, "SIGWINCH"),
            &Signal::SIGIO => write!(f, "SIGNOIO/SIGPOLL"),
            &Signal::SIGPWR => write!(f, "SIGHUPWR"),
            &Signal::SIGSYS => write!(f, "SIGSYS"),
            &Signal(signo @ ..Signal::SIGNUM_MAX) => write!(f, "SIGUSER{}", signo),
            &Signal(signo) => write!(f, "Signal::UNKNOWN({})", signo),
        }
    }
}

#[macro_export]
macro_rules! SIGNAL_IGNORE {
    () => {
        $crate::signal::Signal::SIGCHLD
            | $crate::signal::Signal::SIGURG
            | $crate::signal::Signal::SIGWINCH
    };
}

#[macro_export]
macro_rules! SIGNAL_NOW {
    () => {
        $crate::signal::Signal::SIGKILL | $crate::signal::Signal::SIGSTOP
    };
}

#[macro_export]
macro_rules! SIGNAL_COREDUMP {
    () => {
        $crate::signal::Signal::SIGQUIT
            | $crate::signal::Signal::SIGILL
            | $crate::signal::Signal::SIGABRT
            | $crate::signal::Signal::SIGFPE
            | $crate::signal::Signal::SIGSEGV
            | $crate::signal::Signal::SIGBUS
            | $crate::signal::Signal::SIGTRAP
            | $crate::signal::Signal::SIGSYS
            | $crate::signal::Signal::SIGXCPU
            | $crate::signal::Signal::SIGXFSZ
    };
}

#[macro_export]
macro_rules! SIGNAL_STOP {
    () => {
        $crate::signal::Signal::SIGSTOP
            | $crate::signal::Signal::SIGTSTP
            | $crate::signal::Signal::SIGTTIN
            | $crate::signal::Signal::SIGTTOU
    };
}
