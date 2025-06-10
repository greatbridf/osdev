use core::pin::Pin;
use riscv::register::{
    mhartid,
    sscratch,
    sstatus
};
use sbi::PhysicalAddress;
use eonix_preempt::PreemptGuard;
use eonix_sync_base::LazyLock;

/// TODO:
/// CPU 的一些函数
/// 

#[eonix_percpu::define_percpu]
static LOCAL_CPU: LazyLock<CPU> = LazyLock::new(CPU::new);

use super::{
    config::smp::get_num_harts,
    mm::setup_kernel_satp,
    trap::setup_kernel_trap,
    interrupt::InterruptControl
};

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
        let hart_id = read_hart_id();
        Self {
            hart_id: hart_id,
            interrupt: InterruptControl::new(hart_id),
        }
    }

    /// Load CPU specific configurations for the current Hart.
    ///
    /// # Safety
    /// This function performs low-level hardware initialization and should
    /// only be called once per Hart during its boot sequence.
    pub unsafe fn init(self: Pin<&mut Self>) {
        let self_mut = self.get_unchecked_mut();

        sscratch::write(self_mut.hart_id as usize);

        setup_kernel_trap();

        // CLINT, 10_000 ms
        self_mut.interrupt.setup_timer(10_000);

        // Supervisor Mode Status Register (sstatus)
        // SUM (Supervisor User Memory access): support S-mode access user memory
        // MXR (Make Executable Readable)
        // SIE (Supervisor Interrupt Enable): enable S-mode interrupt
        let mut current_sstatus = sstatus::read();
        current_sstatus.set_spp(sstatus::SPP::Supervisor);
        current_sstatus.set_sum(true);
        current_sstatus.set_mxr(true);
        current_sstatus.set_sie(true);
        sstatus::write(current_sstatus);

        // setup kernel page table and flush tlb
        setup_kernel_satp();
    }

    /// Boot all other hart.
    pub unsafe fn bootstrap_cpus(&self) {
        unsafe extern "C" {
        fn ap_boot_entry();
        }
        let total_harts = get_num_harts();

        let ap_entry_point = PhysicalAddress::new(ap_boot_entry as usize);

        for i in 1..total_harts {
            sbi::hsm::hart_start(i, ap_entry_point, 0)
                .expect("Failed to start secondary hart via SBI");
        }
    }

    pub unsafe fn load_interrupt_stack(self: Pin<&mut Self>, sp: u64) {
        sscratch::write(sp as usize);
    }

    pub fn set_tls32(self: Pin<&mut Self>, user_tls: &UserTLS) {
        // nothing
    }

    pub fn end_of_interrupt(self: Pin<&mut Self>) {
        unsafe {
            // TODO: 不知道写的对不对。。。
            self.map_unchecked_mut(|me| &mut me.interrupt)
                .clear_soft_interrupt_pending();
        }
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

fn read_hart_id() -> usize {
    mhartid::read()
}

#[macro_export]
macro_rules! define_smp_bootstrap {
    ($cpu_count:literal, $ap_entry:ident, $alloc_kstack:tt) => {
        #[no_mangle]
        static BOOT_SEMAPHORE: core::sync::atomic::AtomicU64 =
            core::sync::atomic::AtomicU64::new(0);
        #[no_mangle]
        static BOOT_STACK: core::sync::atomic::AtomicU64 =
            core::sync::atomic::AtomicU64::new(0);

        #[no_mangle]
        static CPU_COUNT: core::sync::atomic::AtomicU64 =
            core::sync::atomic::AtomicU64::new(0);

        core::arch::global_asm!(
            r#"
        .section .text.ap_boot
        .globl ap_boot_entry

        ap_boot_entry:
            csrr a0, mhartid

        1:
            lw t0, AP_BOOT_STACK.addr
            beqz t0, 1b
            li t1, 0
            sw t1, AP_BOOT_STACK.addr
            mv sp, t0

        2:
            lw t0, AP_BOOT_SEMAPHORE.addr
            beqz t0, 2b

            li t1, 0
            sw t1, AP_BOOT_SEMAPHORE.addr

            li t0, 1
            amoswap.w.aq rl t0, a0, ONLINE_HART_COUNT.addr

            call $ap_entry
            j .
            "#,
            BOOT_SEMAPHORE = sym BOOT_SEMAPHORE,
            BOOT_STACK = sym BOOT_STACK,
            CPU_COUNT = sym CPU_COUNT,
            AP_ENTRY = sym $ap_entry,
        );

        pub unsafe fn wait_cpus_online() {
            use core::sync::atomic::Ordering;
            while CPU_COUNT.load(Ordering::Acquire) != $cpu_count - 1 {
                if BOOT_STACK.load(Ordering::Acquire) == 0 {
                    let stack_bottom = $alloc_kstack as u64;
                    BOOT_STACK.store(stack_bottom, Ordering::Release);
                }
                $crate::pause();
            }
        }
    };
}

