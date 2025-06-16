mod trap_context;

use super::config::platform::virt::*;
use super::context::TaskContext;
use core::arch::{global_asm, naked_asm};
use core::mem::{offset_of, size_of};
use core::num::NonZero;
use core::ptr::NonNull;
use eonix_hal_traits::{context::RawTaskContext, trap::TrapReturn};
use riscv::register::sie::Sie;
use riscv::register::stvec::TrapMode;
use riscv::register::{scause, sepc, stval};
use riscv::{
    asm::sfence_vma_all,
    register::{
        sie,
        stvec::{self, Stvec},
    },
};
use sbi::SbiError;

pub use trap_context::*;

#[repr(C)]
pub struct TrapScratch {
    t1: u64,
    t2: u64,
    kernel_tp: Option<NonZero<u64>>,
    trap_context: Option<NonNull<TrapContext>>,
    handler: unsafe extern "C" fn(),
    captured_context: Option<NonNull<TaskContext>>,
    capturer_context: TaskContext,
}

#[eonix_percpu::define_percpu]
pub(crate) static TRAP_SCRATCH: TrapScratch = TrapScratch {
    t1: 0,
    t2: 0,
    kernel_tp: None,
    trap_context: None,
    handler: default_trap_handler,
    captured_context: None,
    capturer_context: TaskContext::new(),
};

#[unsafe(naked)]
unsafe extern "C" fn _raw_trap_entry() -> ! {
    naked_asm!(
        "csrrw t0, sscratch, t0", // Swap t0 and sscratch
        "sd    t1, 0(t0)",
        "sd    t2, 8(t0)",
        "csrr  t1, sstatus",
        "andi  t1, t1, 0x100",
        "beqz  t1, 2f",
        // else SPP = 1, supervisor mode
        "addi  t1, sp, -{trap_context_size}",
        "mv    t2, tp",
        "sd    ra, {ra}(t1)",
        "sd    sp, {sp}(t1)",
        "mv    sp, t1",
        "j     4f",
        // SPP = 0, user mode
        "2:",
        "ld    t1, 24(t0)", // Load captured TrapContext address
        "mv    t2, tp",
        "ld    tp, 16(t0)", // Restore kernel tp
        // t0: &mut TrapScratch, t1: &mut TrapContext, t2: tp before trap
        "3:",
        "sd    ra, {ra}(t1)",
        "sd    sp, {sp}(t1)",
        "4:",
        "sd    gp, {gp}(t1)",
        "sd    t2, {tp}(t1)",
        "ld    ra, 0(t0)",
        "ld    t2, 8(t0)",
        "sd    ra, {t1}(t1)",     // Save t1
        "sd    t2, {t2}(t1)",     // Save t2
        "ld    ra, 32(t0)",       // Load handler address
        "csrrw t2, sscratch, t0", // Swap t0 and sscratch
        "sd    t2, {t0}(t1)",
        "sd    a0, {a0}(t1)",
        "sd    a1, {a1}(t1)",
        "sd    a2, {a2}(t1)",
        "sd    a3, {a3}(t1)",
        "sd    a4, {a4}(t1)",
        "sd    a5, {a5}(t1)",
        "sd    a6, {a6}(t1)",
        "sd    a7, {a7}(t1)",
        "sd    t3, {t3}(t1)",
        "sd    t4, {t4}(t1)",
        "sd    t5, {t5}(t1)",
        "sd    t6, {t6}(t1)",
        "csrr  t2, sstatus",
        "csrr  t3, sepc",
        "csrr  t4, scause",
        "sd    t2, {sstatus}(t1)",
        "sd    t3, {sepc}(t1)",
        "sd    t4, {scause}(t1)",
        "ret",
        trap_context_size = const size_of::<TrapContext>(),
        ra = const Registers::OFFSET_RA,
        sp = const Registers::OFFSET_SP,
        gp = const Registers::OFFSET_GP,
        tp = const Registers::OFFSET_TP,
        t1 = const Registers::OFFSET_T1,
        t2 = const Registers::OFFSET_T2,
        t0 = const Registers::OFFSET_T0,
        a0 = const Registers::OFFSET_A0,
        a1 = const Registers::OFFSET_A1,
        a2 = const Registers::OFFSET_A2,
        a3 = const Registers::OFFSET_A3,
        a4 = const Registers::OFFSET_A4,
        a5 = const Registers::OFFSET_A5,
        a6 = const Registers::OFFSET_A6,
        a7 = const Registers::OFFSET_A7,
        t3 = const Registers::OFFSET_T3,
        t4 = const Registers::OFFSET_T4,
        t5 = const Registers::OFFSET_T5,
        t6 = const Registers::OFFSET_T6,
        sstatus = const TrapContext::OFFSET_SSTATUS,
        sepc = const TrapContext::OFFSET_SEPC,
        scause = const TrapContext::OFFSET_SCAUSE,
    );
}

#[unsafe(naked)]
unsafe extern "C" fn _raw_trap_return(ctx: &mut TrapContext) -> ! {
    naked_asm!(
        "ld ra, {ra}(a0)",
        "ld sp, {sp}(a0)",
        "ld gp, {gp}(a0)",
        "ld tp, {tp}(a0)",
        "ld t1, {t1}(a0)",
        "ld t2, {t2}(a0)",
        "ld t0, {t0}(a0)",
        "ld a1, {a1}(a0)",
        "ld a2, {a2}(a0)",
        "ld a3, {a3}(a0)",
        "ld a4, {a4}(a0)",
        "ld a5, {a5}(a0)",
        "ld a6, {a6}(a0)",
        "ld a7, {a7}(a0)",
        "ld t3, {t3}(a0)",
        "ld t4, {sepc}(a0)",    // Load sepc from TrapContext
        "ld t5, {sstatus}(a0)", // Load sstatus from TrapContext
        "csrw sepc, t4",        // Restore sepc
        "csrw sstatus, t5",     // Restore sstatus
        "ld t4, {t4}(a0)",
        "ld t5, {t5}(a0)",
        "ld t6, {t6}(a0)",
        "ld a0, {a0}(a0)",
        "sret",
        ra = const Registers::OFFSET_RA,
        sp = const Registers::OFFSET_SP,
        gp = const Registers::OFFSET_GP,
        tp = const Registers::OFFSET_TP,
        t1 = const Registers::OFFSET_T1,
        t2 = const Registers::OFFSET_T2,
        t0 = const Registers::OFFSET_T0,
        a0 = const Registers::OFFSET_A0,
        a1 = const Registers::OFFSET_A1,
        a2 = const Registers::OFFSET_A2,
        a3 = const Registers::OFFSET_A3,
        a4 = const Registers::OFFSET_A4,
        a5 = const Registers::OFFSET_A5,
        a6 = const Registers::OFFSET_A6,
        a7 = const Registers::OFFSET_A7,
        t3 = const Registers::OFFSET_T3,
        t4 = const Registers::OFFSET_T4,
        t5 = const Registers::OFFSET_T5,
        t6 = const Registers::OFFSET_T6,
        sstatus = const TrapContext::OFFSET_SSTATUS,
        sepc = const TrapContext::OFFSET_SEPC,
    );
}

#[unsafe(naked)]
unsafe extern "C" fn default_trap_handler() {
    unsafe extern "C" {
        fn _default_trap_handler(trap_context: &mut TrapContext);
    }

    naked_asm!(
        "andi sp, sp, -16", // Align stack pointer to 16 bytes
        "addi sp, sp, -16",
        "mv   a0, t1",      // TrapContext pointer in t1
        "sd   a0, 0(sp)",   // Save TrapContext pointer
        "",
        "call {default_handler}",
        "",
        "ld   a0, 0(sp)",   // Restore TrapContext pointer
        "j {trap_return}",
        default_handler = sym _default_trap_handler,
        trap_return = sym _raw_trap_return,
    );
}

#[unsafe(naked)]
unsafe extern "C" fn captured_trap_handler() {
    naked_asm!(
        "ld   a0, {captured_context_offset}(t0)",
        "addi a1, t0, {capturer_context_offset}",
        "j {switch}",
        captured_context_offset = const offset_of!(TrapScratch, captured_context),
        capturer_context_offset = const offset_of!(TrapScratch, capturer_context),
        switch = sym TaskContext::switch,
    );
}

#[unsafe(naked)]
unsafe extern "C" fn captured_trap_return(trap_context: usize) -> ! {
    naked_asm!(
        "mv a0, sp",
        "j {raw_trap_return}",
        raw_trap_return = sym _raw_trap_return,
    );
}

impl TrapScratch {
    pub fn set_trap_context(&mut self, ctx: NonNull<TrapContext>) {
        self.trap_context = Some(ctx);
    }

    pub fn clear_trap_context(&mut self) {
        self.trap_context = None;
    }

    pub fn set_kernel_tp(&mut self, tp: NonNull<u8>) {
        self.kernel_tp = Some(NonZero::new(tp.addr().get() as u64).unwrap());
    }
}

impl TrapReturn for TrapContext {
    type TaskContext = TaskContext;

    unsafe fn trap_return(&mut self, to_ctx: &mut Self::TaskContext) {
        let irq_states = disable_irqs_save();
        let old_handler = {
            let trap_scratch = TRAP_SCRATCH.as_mut();
            trap_scratch.captured_context = Some(NonNull::from(&mut *to_ctx));
            core::mem::replace(&mut trap_scratch.handler, captured_trap_handler)
        };

        to_ctx.set_program_counter(captured_trap_return as usize);
        to_ctx.set_stack_pointer(&raw mut *self as usize);
        to_ctx.set_interrupt_enabled(false);

        unsafe {
            TaskContext::switch(&mut TRAP_SCRATCH.as_mut().capturer_context, to_ctx);
        }

        {
            let trap_scratch = TRAP_SCRATCH.as_mut();
            trap_scratch.handler = old_handler;
            trap_scratch.captured_context = None;
        }

        irq_states.restore();
    }
}

fn setup_trap_handler(trap_entry_addr: usize) {
    let mut stvec_val = Stvec::from_bits(0);
    stvec_val.set_address(trap_entry_addr);
    stvec_val.set_trap_mode(TrapMode::Direct);

    unsafe {
        stvec::write(stvec_val);
    }
}

pub fn setup_trap() {
    setup_trap_handler(_raw_trap_entry as usize);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqState(Sie);

impl IrqState {
    #[inline]
    pub fn save() -> Self {
        IrqState(sie::read())
    }

    #[inline]
    pub fn restore(self) {
        let Self(state) = self;
        unsafe {
            sie::write(state);
        }
    }
}

#[inline]
pub fn disable_irqs() {
    unsafe {
        sie::clear_sext();
        sie::clear_stimer();
        sie::clear_ssoft();
    }
}

#[inline]
pub fn enable_irqs() {
    unsafe {
        sie::set_sext();
        sie::set_stimer();
        sie::set_ssoft();
    }
}

#[inline]
pub fn disable_irqs_save() -> IrqState {
    let state = IrqState::save();
    disable_irqs();

    state
}
