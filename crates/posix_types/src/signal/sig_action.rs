use super::Signal;
use crate::ctypes::PtrT;
use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct SigActionFlags: usize {
        const SA_SIGINFO = 0x00000004;   // Use sa_sigaction instead of sa_handler.
        const SA_RESTORER = 0x04000000; // Use sa_restorer to restore the context after the handler.
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigActionHandler(PtrT);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigActionRestorer(PtrT);

#[cfg_attr(target_arch = "x86_64", repr(align(4)))]
#[cfg_attr(not(target_arch = "x86_64"), repr(align(8)))]
#[derive(Debug, Clone, Copy, Default)]
pub struct SigSet(u64);

#[cfg(target_arch = "x86_64")]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SigAction {
    sa_handler: SigActionHandler,
    sa_flags: SigActionFlags,
    sa_restorer: SigActionRestorer,
    sa_mask: SigSet,
}

#[cfg(not(target_arch = "x86_64"))]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SigAction {
    sa_handler: SigActionHandler,
    sa_flags: SigActionFlags,
    sa_mask: SigSet,
}

pub trait TryFromSigAction: Sized {
    type Error;

    fn default() -> Self;
    fn ignore() -> Self;
    fn new() -> Self;

    fn set_siginfo(self) -> Result<Self, Self::Error>;
    fn handler(self, handler: SigActionHandler) -> Self;
    fn restorer(self, restorer: SigActionRestorer) -> Self;
    fn mask(self, mask: SigSet) -> Self;
}

impl SigActionHandler {
    const DEFAULT: Self = Self(PtrT::new_val(0));
    const IGNORE: Self = Self(PtrT::new_val(1));

    pub const fn new(handler: PtrT) -> Self {
        Self(handler)
    }

    pub const fn null() -> Self {
        Self(PtrT::null())
    }

    pub const fn addr(self) -> PtrT {
        self.0
    }
}

impl SigActionRestorer {
    pub const fn new(restorer: PtrT) -> Self {
        Self(restorer)
    }

    pub const fn null() -> Self {
        Self(PtrT::null())
    }

    pub const fn addr(self) -> PtrT {
        self.0
    }
}

impl SigSet {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub fn mask(&mut self, mask: Self) {
        self.0 |= mask.0;
    }

    pub fn unmask(&mut self, mask: Self) {
        self.0 &= !mask.0;
    }

    pub fn include(&self, signal: Signal) -> bool {
        let signal_mask = Self::from(signal);
        (self.0 & signal_mask.0) != 0
    }
}

impl From<Signal> for SigSet {
    fn from(signal: Signal) -> Self {
        let mut sigset = Self::empty();
        sigset.mask(Self(1 << (signal.into_raw() - 1)));
        sigset
    }
}

impl SigAction {
    pub const fn default() -> Self {
        Self {
            sa_handler: SigActionHandler::DEFAULT,
            sa_flags: SigActionFlags::empty(),
            #[cfg(target_arch = "x86_64")]
            sa_restorer: SigActionRestorer::null(),
            sa_mask: SigSet::empty(),
        }
    }

    pub const fn ignore() -> Self {
        Self {
            sa_handler: SigActionHandler::IGNORE,
            sa_flags: SigActionFlags::empty(),
            #[cfg(target_arch = "x86_64")]
            sa_restorer: SigActionRestorer::null(),
            sa_mask: SigSet::empty(),
        }
    }

    pub const fn new() -> Self {
        Self {
            sa_handler: SigActionHandler::null(),
            sa_flags: SigActionFlags::empty(),
            #[cfg(target_arch = "x86_64")]
            sa_restorer: SigActionRestorer::null(),
            sa_mask: SigSet::empty(),
        }
    }

    pub fn handler(self, handler: SigActionHandler) -> Self {
        Self {
            sa_handler: handler,
            ..self
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    pub fn restorer(self, _restorer: SigActionRestorer) -> Self {
        // On non-x86_64 architectures, the restorer does not exist.
        self
    }

    #[cfg(target_arch = "x86_64")]
    pub fn restorer(mut self, restorer: SigActionRestorer) -> Self {
        self.sa_flags.insert(SigActionFlags::SA_RESTORER);

        Self {
            sa_restorer: restorer,
            ..self
        }
    }

    pub const fn mask(self, mask: SigSet) -> Self {
        Self {
            sa_mask: mask,
            ..self
        }
    }

    pub fn try_into<T>(self) -> Result<T, T::Error>
    where
        T: TryFromSigAction,
    {
        match self.sa_handler {
            SigActionHandler::DEFAULT => Ok(T::default()),
            SigActionHandler::IGNORE => Ok(T::ignore()),
            _ => {
                let mut action = T::new();
                if self.sa_flags.contains(SigActionFlags::SA_SIGINFO) {
                    action = action.set_siginfo()?;
                }

                action = action.handler(self.sa_handler);
                action = action.mask(self.sa_mask);

                #[cfg(target_arch = "x86_64")]
                {
                    action = action.restorer(self.sa_restorer);
                }

                Ok(action)
            }
        }
    }
}
