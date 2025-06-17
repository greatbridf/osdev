use super::{KResult, Signal, SignalMask, SAVED_DATA_SIZE};
use crate::{
    io::BufferFill as _,
    kernel::{
        constants::{EFAULT, EINVAL, ENOSYS},
        user::UserBuffer,
    },
    SIGNAL_NOW,
};
use alloc::{collections::btree_map::BTreeMap, sync::Arc};
use arch::FpuState;
use core::num::NonZero;
use eonix_hal::{traits::trap::RawTrapContext, trap::TrapContext};
use eonix_mm::address::{Addr as _, AddrOps as _, VAddr};
use eonix_sync::Spin;
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
    actions: Spin<BTreeMap<Signal, SignalAction>>,
}

impl SignalActionList {
    pub fn new_shared(other: &Arc<Self>) -> Arc<Self> {
        other.clone()
    }

    pub fn new_cloned(other: &Self) -> Arc<Self> {
        Arc::new(Self {
            actions: Spin::new(other.actions.lock().clone()),
        })
    }
}

impl SignalActionList {
    pub const fn new() -> Self {
        Self {
            actions: Spin::new(BTreeMap::new()),
        }
    }

    pub fn set(&self, signal: Signal, action: SignalAction) {
        debug_assert!(
            !matches!(signal, SIGNAL_NOW!()),
            "SIGSTOP and SIGKILL should not be set for a handler."
        );
        match action {
            SignalAction::Default => self.actions.lock().remove(&signal),
            _ => self.actions.lock().insert(signal, action),
        };
    }

    pub fn get(&self, signal: Signal) -> SignalAction {
        match self.actions.lock().get(&signal) {
            None => SignalAction::Default,
            Some(action) => action.clone(),
        }
    }

    pub fn remove_non_ignore(&self) {
        // Remove all custom handlers except for the ignore action.
        // Default handlers should never appear in the list so we don't consider that.
        self.actions
            .lock()
            .retain(|_, action| matches!(action, SignalAction::Ignore));
    }
}

impl SignalAction {
    /// # Might Sleep
    pub(super) fn handle(
        self,
        signal: Signal,
        old_mask: SignalMask,
        trap_ctx: &mut TrapContext,
        fpu_state: &mut FpuState,
    ) -> KResult<()> {
        let SignalAction::SimpleHandler {
            handler, restorer, ..
        } = self
        else {
            unreachable!("Default and Ignore actions should not be handled here");
        };

        let Some(restorer) = restorer else {
            // We don't accept signal handlers with no signal restorers for now.
            return Err(ENOSYS)?;
        };

        let current_sp = VAddr::from(trap_ctx.get_stack_pointer());

        // Save current interrupt context to 128 bytes above current user stack
        // (in order to keep away from x86 red zone) and align to 16 bytes.
        let saved_data_addr = (current_sp - 128 - SAVED_DATA_SIZE).floor_to(16);

        let mut saved_data_buffer =
            UserBuffer::new(saved_data_addr.addr() as *mut u8, SAVED_DATA_SIZE)?;

        saved_data_buffer.copy(trap_ctx)?.ok_or(EFAULT)?;
        saved_data_buffer.copy(fpu_state)?.ok_or(EFAULT)?;
        saved_data_buffer.copy(&old_mask)?.ok_or(EFAULT)?;

        // We need to push the arguments to the handler and then save the return address.
        let new_sp = saved_data_addr - 16 - 4;
        let restorer_address = restorer.addr() as u32;

        let mut stack = UserBuffer::new(new_sp.addr() as *mut u8, 4 + 4)?;
        stack.copy(&restorer_address)?.ok_or(EFAULT)?; // Restorer address
        stack.copy(&u32::from(signal))?.ok_or(EFAULT)?; // The argument to the handler

        trap_ctx.set_program_counter(handler.addr());
        trap_ctx.set_stack_pointer(new_sp.addr());
        Ok(())
    }
}

impl Clone for SignalActionList {
    fn clone(&self) -> Self {
        Self {
            actions: Spin::new(self.actions.lock().clone()),
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
            handler: VAddr::NULL,
            restorer: None,
            mask: SignalMask::empty(),
        }
    }

    fn set_siginfo(self) -> Result<Self, Self::Error> {
        Err(EINVAL)
    }

    fn handler(mut self, handler_addr: usize) -> Result<Self, Self::Error> {
        if let Self::SimpleHandler { handler, .. } = &mut self {
            *handler = VAddr::from(handler_addr);
            Ok(self)
        } else {
            unreachable!()
        }
    }

    fn restorer(mut self, restorer_addr: usize) -> Result<Self, Self::Error> {
        if let Self::SimpleHandler { restorer, .. } = &mut self {
            *restorer = NonZero::new(restorer_addr).map(|x| VAddr::from(x.get()));
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
                    .handler(handler.addr())
                    .mask(u64::from(mask));

                if let Some(restorer) = restorer {
                    action.restorer(restorer.addr())
                } else {
                    action
                }
            }
        }
    }
}
