use core::arch::asm;
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::pin::Pin;
use core::ptr::NonNull;
use core::sync::atomic::AtomicUsize;

use eonix_hal_traits::trap::RawTrapContext;
use eonix_preempt::PreemptGuard;
use eonix_sync_base::LazyLock;
use riscv::register::medeleg::{self, Medeleg};
use riscv::register::{mhartid, sscratch, sstatus};
use sbi::PhysicalAddress;

use super::interrupt::InterruptControl;
use super::trap::{setup_trap, TrapContext};
use crate::arch::fdt::{FdtExt, FDT};

pub static CPU_COUNT: AtomicUsize = AtomicUsize::new(0);

#[eonix_percpu::define_percpu]
pub static CPUID: usize = 0;

#[eonix_percpu::define_percpu]
static DEFAULT_TRAP_CONTEXT: MaybeUninit<TrapContext> = MaybeUninit::uninit();

#[eonix_percpu::define_percpu]
static LOCAL_CPU: LazyLock<CPU> = LazyLock::new(|| CPU::new(CPUID.get()));

/// RISC-V Hart
pub struct CPU {
    pub(crate) interrupt: InterruptControl,
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
