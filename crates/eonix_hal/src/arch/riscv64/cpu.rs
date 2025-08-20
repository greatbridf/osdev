use super::{
    interrupt::InterruptControl,
    trap::{setup_trap, TrapContext},
};
use core::{
    arch::asm, cell::UnsafeCell, mem::MaybeUninit, pin::Pin, ptr::NonNull,
    sync::atomic::AtomicUsize,
};
use eonix_hal_traits::trap::RawTrapContext;
use eonix_preempt::PreemptGuard;
use eonix_sync_base::LazyLock;
use riscv::register::{
    medeleg::{self, Medeleg},
    mhartid, sscratch, sstatus,
};
use sbi::PhysicalAddress;

pub static CPU_COUNT: AtomicUsize = AtomicUsize::new(0);

#[eonix_percpu::define_percpu]
pub static CPUID: usize = 0;

#[eonix_percpu::define_percpu]
static DEFAULT_TRAP_CONTEXT: MaybeUninit<TrapContext> = MaybeUninit::uninit();

#[eonix_percpu::define_percpu]
static LOCAL_CPU: LazyLock<CPU> = LazyLock::new(|| CPU::new(CPUID.get()));

#[derive(Debug, Clone)]
pub enum UserTLS {
    Base(u64),
}

/// RISC-V Hart
pub struct CPU {
    pub(crate) interrupt: InterruptControl,
}

impl UserTLS {
    pub fn new(base: u64) -> Self {
        Self::Base(base)
    }
}

impl CPU {
    fn new(cpuid: usize) -> Self {
        Self {
            interrupt: InterruptControl::new(cpuid),
        }
    }

    /// Load CPU specific configurations for the current Hart.
    ///
    /// # Safety
    /// This function performs low-level hardware initialization and should
    /// only be called once per Hart during its boot sequence.
    pub unsafe fn init(mut self: Pin<&mut Self>) {
        let me = self.as_mut().get_unchecked_mut();
        setup_trap();

        let interrupt = self.map_unchecked_mut(|me| &mut me.interrupt);
        interrupt.init();

        sstatus::set_sum();
        sscratch::write(DEFAULT_TRAP_CONTEXT.as_ptr() as usize);
    }

    pub unsafe fn load_interrupt_stack(self: Pin<&mut Self>, sp: u64) {}

    pub fn set_tls32(self: Pin<&mut Self>, _user_tls: &UserTLS) {
        // nothing
    }

    pub fn local() -> PreemptGuard<Pin<&'static mut Self>> {
        unsafe {
            // SAFETY: We pass the reference into a `PreemptGuard`, which ensures
            //         that preemption is disabled.
            PreemptGuard::new(Pin::new_unchecked(LOCAL_CPU.as_mut().get_mut()))
        }
    }

    pub fn cpuid(&self) -> usize {
        CPUID.get()
    }
}

#[inline(always)]
pub fn halt() {
    unsafe {
        asm!("wfi", options(nomem, nostack, preserves_flags));
    }
}
