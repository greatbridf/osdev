use super::{KResult, SAVED_DATA_SIZE};
use crate::{
    io::BufferFill as _,
    kernel::{
        constants::{EFAULT, EINVAL},
        syscall::UserMut,
        user::UserBuffer,
    },
};
use alloc::{collections::btree_map::BTreeMap, sync::Arc};
use core::arch::naked_asm;
use eonix_hal::{fpu::FpuState, traits::trap::RawTrapContext, trap::TrapContext};
use eonix_mm::address::{Addr as _, AddrOps as _, VAddr};
use eonix_sync::Spin;
use posix_types::{
    ctypes::Long,
    signal::{SigAction, SigActionHandler, SigActionRestorer, SigSet, Signal, TryFromSigAction},
    SIGNAL_NOW,
};

#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
#[unsafe(link_section = ".vdso.sigreturn")]
unsafe extern "C" fn vdso_sigreturn() {
    naked_asm!(
        "pop %rax",
        "mov ${sys_sigreturn}, %eax",
        "int $0x80",
        sys_sigreturn = const posix_types::syscall_no::SYS_SIGRETURN,
        options(att_syntax),
    );
}

#[unsafe(naked)]
#[unsafe(link_section = ".vdso.rt_sigreturn")]
unsafe extern "C" fn vdso_rt_sigreturn() {
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "riscv64",
        target_arch = "loongarch64"
    )))]
    compile_error!("rt_sigreturn is not implemented for this architecture");

    #[cfg(target_arch = "riscv64")]
    naked_asm!(
        "li a7, {sys_rt_sigreturn}",
        "ecall",
        sys_rt_sigreturn = const posix_types::syscall_no::SYS_RT_SIGRETURN,
    );

    #[cfg(target_arch = "loongarch64")]
    naked_asm!(
        "li.d $a7, {sys_rt_sigreturn}",
        "syscall 0",
        sys_rt_sigreturn = const posix_types::syscall_no::SYS_RT_SIGRETURN,
    );

    #[cfg(target_arch = "x86_64")]
    naked_asm!(
        "mov ${sys_rt_sigreturn}, %eax",
        "int $0x80",
        sys_rt_sigreturn = const posix_types::syscall_no::SYS_RT_SIGRETURN,
        options(att_syntax),
    );
}

#[derive(Debug, Clone, Copy)]
pub enum SignalAction {
    Default,
    Ignore,
    SimpleHandler {
        handler: SigActionHandler,
        restorer: Option<SigActionRestorer>,
        mask: SigSet,
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
        old_mask: SigSet,
        trap_ctx: &mut TrapContext,
        fpu_state: &mut FpuState,
    ) -> KResult<()> {
        let SignalAction::SimpleHandler {
            handler, restorer, ..
        } = self
        else {
            unreachable!("Default and Ignore actions should not be handled here");
        };

        let current_sp = VAddr::from(trap_ctx.get_stack_pointer());

        #[cfg(target_arch = "x86_64")]
        // Save current interrupt context to 128 bytes above current user stack
        // (in order to keep away from x86 red zone) and align to 16 bytes.
        let saved_data_addr = (current_sp - 128 - SAVED_DATA_SIZE).floor_to(16);

        #[cfg(not(target_arch = "x86_64"))]
        let saved_data_addr = (current_sp - SAVED_DATA_SIZE).floor_to(16);

        let mut saved_data_buffer =
            UserBuffer::new(UserMut::new(saved_data_addr), SAVED_DATA_SIZE)?;

        saved_data_buffer.copy(trap_ctx)?.ok_or(EFAULT)?;
        saved_data_buffer.copy(fpu_state)?.ok_or(EFAULT)?;
        saved_data_buffer.copy(&old_mask)?.ok_or(EFAULT)?;

        let return_address = if let Some(restorer) = restorer {
            restorer.addr().addr()
        } else {
            #[cfg(not(any(
                target_arch = "x86_64",
                target_arch = "riscv64",
                target_arch = "loongarch64"
            )))]
            compile_error!("`vdso_sigreturn` is not implemented for this architecture");

            #[cfg(target_arch = "x86_64")]
            {
                // TODO: Check and use `vdso_rt_sigreturn` for x86 as well.
                static VDSO_SIGRETURN_ADDR: &'static unsafe extern "C" fn() =
                    &(vdso_rt_sigreturn as unsafe extern "C" fn());

                unsafe {
                    // SAFETY: To prevent the compiler from optimizing this into `la` instructions
                    //         and causing a linking error.
                    (VDSO_SIGRETURN_ADDR as *const _ as *const usize).read_volatile()
                }
            }

            #[cfg(any(target_arch = "riscv64", target_arch = "loongarch64"))]
            {
                static VDSO_RT_SIGRETURN_ADDR: &'static unsafe extern "C" fn() =
                    &(vdso_rt_sigreturn as unsafe extern "C" fn());

                unsafe {
                    // SAFETY: To prevent the compiler from optimizing this into `la` instructions
                    //         and causing a linking error.
                    (VDSO_RT_SIGRETURN_ADDR as *const _ as *const usize).read_volatile()
                }
            }
        };

        trap_ctx.set_user_call_frame(
            handler.addr().addr(),
            Some(saved_data_addr.addr()),
            Some(return_address),
            &[Long::new_val(signal.into_raw() as _).get()],
            |vaddr, data| -> Result<(), u32> {
                let mut buffer = UserBuffer::new(UserMut::new(vaddr), data.len())?;
                for ch in data.iter() {
                    buffer.copy(&ch)?.ok_or(EFAULT)?;
                }

                Ok(())
            },
        )?;

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
            handler: SigActionHandler::null(),
            restorer: None,
            mask: SigSet::empty(),
        }
    }

    fn set_siginfo(self) -> Result<Self, Self::Error> {
        Err(EINVAL)
    }

    fn handler(mut self, new_handler: SigActionHandler) -> Self {
        if let Self::SimpleHandler { handler, .. } = &mut self {
            *handler = new_handler;
            self
        } else {
            unreachable!()
        }
    }

    fn restorer(mut self, new_restorer: SigActionRestorer) -> Self {
        if let Self::SimpleHandler { restorer, .. } = &mut self {
            *restorer = Some(new_restorer);
            self
        } else {
            unreachable!()
        }
    }

    fn mask(mut self, new_mask: SigSet) -> Self {
        if let Self::SimpleHandler { mask, .. } = &mut self {
            *mask = new_mask;
            self
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
                let action = SigAction::new().handler(handler).mask(mask);

                if let Some(restorer) = restorer {
                    action.restorer(restorer)
                } else {
                    action
                }
            }
        }
    }
}
