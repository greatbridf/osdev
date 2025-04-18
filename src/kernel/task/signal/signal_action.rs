use super::{KResult, Signal, SignalMask};
use crate::{
    io::BufferFill as _,
    kernel::{
        constants::{EFAULT, EINVAL, ENOSYS},
        mem::VAddr,
        user::UserBuffer,
    },
    SIGNAL_NOW,
};
use alloc::collections::btree_map::BTreeMap;
use arch::{ExtendedContext, InterruptContext};
use core::num::NonZero;
use posix_types::signal::{SigAction, TryFromSigAction};

#[derive(Debug, Clone, Copy)]
pub enum SignalAction {
    Default,
    Ignore,
    SimpleHandler {
        handler: VAddr,
        restorer: Option<VAddr>,
        mask: SignalMask,
    },
}

#[derive(Debug)]
pub struct SignalActionList {
    actions: BTreeMap<Signal, SignalAction>,
}

impl SignalActionList {
    pub const fn new() -> Self {
        Self {
            actions: BTreeMap::new(),
        }
    }

    pub fn set(&mut self, signal: Signal, action: SignalAction) {
        debug_assert!(
            !matches!(signal, SIGNAL_NOW!()),
            "SIGSTOP and SIGKILL should not be set for a handler."
        );
        match action {
            SignalAction::Default => self.actions.remove(&signal),
            _ => self.actions.insert(signal, action),
        };
    }

    pub fn get(&self, signal: Signal) -> SignalAction {
        match self.actions.get(&signal) {
            None => SignalAction::Default,
            Some(action) => action.clone(),
        }
    }

    pub fn remove_non_ignore(&mut self) {
        // Remove all custom handlers except for the ignore action.
        // Default handlers should never appear in the list so we don't consider that.
        self.actions
            .retain(|_, action| matches!(action, SignalAction::Ignore));
    }
}

impl SignalAction {
    /// # Might Sleep
    pub(super) fn handle(
        self,
        signal: Signal,
        old_mask: SignalMask,
        int_stack: &mut InterruptContext,
        ext_ctx: &mut ExtendedContext,
    ) -> KResult<()> {
        // TODO: The sizes of the context structures should be arch-specific.
        const CONTEXT_SIZE: usize = size_of::<InterruptContext>()
            + size_of::<ExtendedContext>()
            + size_of::<SignalMask>() // old_mask
            + size_of::<u32>(); // `sa_handler` argument: `signum`

        match self {
            SignalAction::Default | SignalAction::Ignore => {
                unreachable!("Default and Ignore actions should not be handled here")
            }
            // We don't accept signal handlers with no signal restorers for now.
            SignalAction::SimpleHandler { restorer: None, .. } => Err(ENOSYS),
            SignalAction::SimpleHandler {
                handler,
                restorer: Some(restorer),
                ..
            } => {
                // Save current interrupt context to 128 bytes above current user stack
                // and align to 16 bytes. Then we push the return address of the restorer.
                // TODO!!!: Determine the size of the return address
                let sp = VAddr::from(int_stack.rsp as usize - 128 - CONTEXT_SIZE).floor_to(16)
                    - size_of::<u32>();
                let restorer_address = usize::from(restorer) as u32;
                let mut stack =
                    UserBuffer::new(usize::from(sp) as *mut u8, CONTEXT_SIZE + size_of::<u32>())?;

                stack.copy(&restorer_address)?.ok_or(EFAULT)?; // Restorer address
                stack.copy(&u32::from(signal))?.ok_or(EFAULT)?; // `signum`
                stack.copy(&old_mask)?.ok_or(EFAULT)?; // Original signal mask
                stack.copy(ext_ctx)?.ok_or(EFAULT)?; // MMX registers
                stack.copy(int_stack)?.ok_or(EFAULT)?; // Interrupt stack

                int_stack.rip = usize::from(handler) as u64;
                int_stack.rsp = usize::from(sp) as u64;
                Ok(())
            }
        }
    }
}

impl Clone for SignalActionList {
    fn clone(&self) -> Self {
        Self {
            actions: self.actions.clone(),
        }
    }
}

impl Default for SignalAction {
    fn default() -> Self {
        Self::Default
    }
}

impl TryFromSigAction for SignalAction {
    type Error = u32;

    fn default() -> Self {
        Self::Default
    }

    fn ignore() -> Self {
        Self::Ignore
    }

    fn new() -> Self {
        Self::SimpleHandler {
            handler: VAddr(0),
            restorer: None,
            mask: SignalMask::empty(),
        }
    }

    fn set_siginfo(self) -> Result<Self, Self::Error> {
        Err(EINVAL)
    }

    fn handler(mut self, handler_addr: usize) -> Result<Self, Self::Error> {
        if let Self::SimpleHandler { handler, .. } = &mut self {
            *handler = VAddr(handler_addr);
            Ok(self)
        } else {
            unreachable!()
        }
    }

    fn restorer(mut self, restorer_addr: usize) -> Result<Self, Self::Error> {
        if let Self::SimpleHandler { restorer, .. } = &mut self {
            *restorer = NonZero::new(restorer_addr).map(|x| VAddr(x.get()));
            Ok(self)
        } else {
            unreachable!()
        }
    }

    fn mask(mut self, mask_value: u64) -> Result<Self, Self::Error> {
        if let Self::SimpleHandler { mask, .. } = &mut self {
            *mask = SignalMask::from(mask_value);
            Ok(self)
        } else {
            unreachable!()
        }
    }
}

impl From<SignalAction> for SigAction {
    fn from(action: SignalAction) -> SigAction {
        match action {
            SignalAction::Default => SigAction::default(),
            SignalAction::Ignore => SigAction::ignore(),
            SignalAction::SimpleHandler {
                handler,
                restorer,
                mask,
            } => {
                let action = SigAction::new()
                    .handler(usize::from(handler))
                    .mask(u64::from(mask));

                if let Some(restorer) = restorer {
                    action.restorer(usize::from(restorer))
                } else {
                    action
                }
            }
        }
    }
}
