use super::trap::setup_trap;
use core::sync::atomic::AtomicUsize;
use core::{arch::asm, pin::Pin, ptr::NonNull};
use eonix_preempt::PreemptGuard;
use eonix_sync_base::LazyLock;

pub static CPU_COUNT: AtomicUsize = AtomicUsize::new(0);

#[eonix_percpu::define_percpu]
pub static CPUID: usize = 0;

#[eonix_percpu::define_percpu]
static LOCAL_CPU: LazyLock<CPU> = LazyLock::new(|| CPU::new(CPUID.get()));

#[derive(Debug, Clone)]
pub enum UserTLS {
    Base(u64),
}

pub struct CPU {}

impl UserTLS {
    pub fn new(base: u64) -> Self {
        Self::Base(base)
    }
}

impl CPU {
    fn new(cpuid: usize) -> Self {
        Self {}
    }

    /// Load CPU specific configurations for the current Hart.
    ///
    /// # Safety
    /// This function performs low-level hardware initialization and should
    /// only be called once per Hart during its boot sequence.
    pub unsafe fn init(mut self: Pin<&mut Self>) {
        let me = self.as_mut().get_unchecked_mut();
        setup_trap();
    }

    /// Boot all other hart.
    pub unsafe fn bootstrap_cpus(&self) {
        // todo
    }

    pub unsafe fn load_interrupt_stack(self: Pin<&mut Self>, _: u64) {}

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
        loongArch64::asm::idle();
    }
}
