#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct SigAction {
    sa_handler: u32,
    sa_flags: u32,
    sa_restorer: u32,
    sa_mask: u64,
}

pub trait TryFromSigAction: Sized {
    type Error;

    fn default() -> Self;
    fn ignore() -> Self;
    fn new() -> Self;

    fn set_siginfo(self) -> Result<Self, Self::Error>;
    fn handler(self, handler: usize) -> Result<Self, Self::Error>;
    fn restorer(self, restorer: usize) -> Result<Self, Self::Error>;
    fn mask(self, mask: u64) -> Result<Self, Self::Error>;
}

const SIG_DFL: u32 = 0;
const SIG_IGN: u32 = 1;

const SA_SIGINFO: u32 = 4;
const SA_RESTORER: u32 = 0x04000000;

impl SigAction {
    pub const fn default() -> Self {
        Self {
            sa_handler: SIG_DFL,
            sa_flags: 0,
            sa_restorer: 0,
            sa_mask: 0,
        }
    }

    pub const fn ignore() -> Self {
        Self {
            sa_handler: SIG_IGN,
            sa_flags: 0,
            sa_restorer: 0,
            sa_mask: 0,
        }
    }

    pub const fn new() -> Self {
        Self {
            sa_handler: 0,
            sa_flags: 0,
            sa_restorer: 0,
            sa_mask: 0,
        }
    }

    pub const fn handler(self, handler: usize) -> Self {
        Self {
            sa_handler: handler as u32,
            ..self
        }
    }

    pub const fn restorer(self, restorer: usize) -> Self {
        Self {
            sa_restorer: restorer as u32,
            sa_flags: self.sa_flags | SA_RESTORER,
            ..self
        }
    }

    pub const fn mask(self, mask: u64) -> Self {
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
            SIG_DFL => Ok(T::default()),
            SIG_IGN => Ok(T::ignore()),
            _ => {
                let mut action = T::new();
                if self.sa_flags & SA_SIGINFO != 0 {
                    action = action.set_siginfo()?;
                }

                action = action.handler(self.sa_handler as usize)?;
                action = action.restorer(self.sa_restorer as usize)?;
                action = action.mask(self.sa_mask)?;

                Ok(action)
            }
        }
    }
}

impl Default for SigAction {
    fn default() -> Self {
        Self::default()
    }
}
