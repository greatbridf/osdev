use super::{interrupt::InterruptControl, trap::setup_trap};
use crate::arch::fdt::{FdtExt, FDT};
use core::pin::Pin;
use eonix_preempt::PreemptGuard;
use eonix_sync_base::LazyLock;
use riscv::register::{mhartid, sscratch, sstatus};
use riscv_peripheral::plic::PLIC;
use sbi::PhysicalAddress;

#[eonix_percpu::define_percpu]
static LOCAL_CPU: LazyLock<CPU> = LazyLock::new(CPU::new);

#[derive(Debug, Clone)]
pub enum UserTLS {
    Base(u32),
}

/// RISC-V Hart
pub struct CPU {
    hart_id: usize,
    interrupt: InterruptControl,
}

impl UserTLS {
    #[allow(unused_variables)]
    pub fn new32(base: u32, _limit: u32, _is_limit_in_pages: bool) -> (Self, u32) {
        (Self::Base(base), 0)
    }
}

impl CPU {
    pub fn new() -> Self {
        Self {
            hart_id: 0,
            interrupt: InterruptControl::new(),
        }
    }

    /// Load CPU specific configurations for the current Hart.
    ///
    /// # Safety
    /// This function performs low-level hardware initialization and should
    /// only be called once per Hart during its boot sequence.
    pub unsafe fn init(mut self: Pin<&mut Self>, hart_id: usize) {
        let me = self.as_mut().get_unchecked_mut();
        me.hart_id = hart_id;

        setup_trap();

        let interrupt = self.map_unchecked_mut(|me| &mut me.interrupt);
        interrupt.init();

        let mut current_sstatus = sstatus::read();
        current_sstatus.set_spp(sstatus::SPP::Supervisor);
        current_sstatus.set_sum(true);
        current_sstatus.set_mxr(true);
        sstatus::write(current_sstatus);
    }

    /// Boot all other hart.
    pub unsafe fn bootstrap_cpus(&self) {
        let total_harts = FDT.hart_count();
        for i in (0..total_harts).filter(|&i| i != self.hart_id) {
            sbi::hsm::hart_start(i, todo!("AP entry"), 0)
                .expect("Failed to start secondary hart via SBI");
        }
    }

    pub unsafe fn load_interrupt_stack(self: Pin<&mut Self>, sp: u64) {
        sscratch::write(sp as usize);
    }

    pub fn set_tls32(self: Pin<&mut Self>, _user_tls: &UserTLS) {
        // nothing
    }

    pub fn end_of_interrupt(self: Pin<&mut Self>) {
        todo!()
    }

    pub fn local() -> PreemptGuard<Pin<&'static mut Self>> {
        unsafe {
            // SAFETY: We pass the reference into a `PreemptGuard`, which ensures
            //         that preemption is disabled.
            PreemptGuard::new(Pin::new_unchecked(LOCAL_CPU.as_mut().get_mut()))
        }
    }

    pub fn cpuid(&self) -> usize {
        self.hart_id
    }
}
