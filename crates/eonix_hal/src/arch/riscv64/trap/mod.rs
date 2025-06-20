mod trap_context;

use super::config::platform::virt::*;
use super::context::TaskContext;
use core::arch::{global_asm, naked_asm};
use core::mem::{offset_of, size_of};
use core::num::NonZero;
use core::ptr::NonNull;
use eonix_hal_traits::{
    context::RawTaskContext,
    trap::{IrqState as IrqStateTrait, TrapReturn},
};
use riscv::register::sstatus::{self, Sstatus};
use riscv::register::stvec::TrapMode;
use riscv::register::{scause, sepc, stval};
use riscv::{
    asm::sfence_vma_all,
    register::stvec::{self, Stvec},
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
    capturer_context: TaskContext,
}

#[eonix_percpu::define_percpu]
pub(crate) static TRAP_SCRATCH: TrapScratch = TrapScratch {
    t1: 0,
    t2: 0,
    kernel_tp: None,
    trap_context: None,
    handler: default_trap_handler,
    capturer_context: TaskContext::new(),
};

static mut DIRTY_TASK_CONTEXT: TaskContext = TaskContext::new();

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
        "sd    s0, {s0}(t1)",
        "sd    s1, {s1}(t1)",
        "sd    s2, {s2}(t1)",
        "sd    s3, {s3}(t1)",
        "sd    s4, {s4}(t1)",
        "sd    s5, {s5}(t1)",
        "sd    s6, {s6}(t1)",
        "sd    s7, {s7}(t1)",
        "sd    s8, {s8}(t1)",
        "sd    s9, {s9}(t1)",
        "sd    s10, {s10}(t1)",
        "sd    s11, {s11}(t1)",
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
        s0 = const Registers::OFFSET_S0,
        s1 = const Registers::OFFSET_S1,
        s2 = const Registers::OFFSET_S2,
        s3 = const Registers::OFFSET_S3,
        s4 = const Registers::OFFSET_S4,
        s5 = const Registers::OFFSET_S5,
        s6 = const Registers::OFFSET_S6,
        s7 = const Registers::OFFSET_S7,
        s8 = const Registers::OFFSET_S8,
        s9 = const Registers::OFFSET_S9,
        s10 = const Registers::OFFSET_S10,
        s11 = const Registers::OFFSET_S11,
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
        "ld s0, {s0}(a0)",
        "ld s1, {s1}(a0)",
        "ld s2, {s2}(a0)",
        "ld s3, {s3}(a0)",
        "ld s4, {s4}(a0)",
        "ld s5, {s5}(a0)",
        "ld s6, {s6}(a0)",
        "ld s7, {s7}(a0)",
        "ld s8, {s8}(a0)",
        "ld s9, {s9}(a0)",
        "ld s10, {s10}(a0)",
        "ld s11, {s11}(a0)",
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
        s0 = const Registers::OFFSET_S0,
        s1 = const Registers::OFFSET_S1,
        s2 = const Registers::OFFSET_S2,
        s3 = const Registers::OFFSET_S3,
        s4 = const Registers::OFFSET_S4,
        s5 = const Registers::OFFSET_S5,
        s6 = const Registers::OFFSET_S6,
        s7 = const Registers::OFFSET_S7,
        s8 = const Registers::OFFSET_S8,
        s9 = const Registers::OFFSET_S9,
        s10 = const Registers::OFFSET_S10,
        s11 = const Registers::OFFSET_S11,
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
        "la   a0, {dirty_task_context}",
        "addi a1, t0, {capturer_context_offset}",
        "j {switch}",
        dirty_task_context = sym DIRTY_TASK_CONTEXT,
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

    unsafe fn trap_return(&mut self) {
        let irq_states = disable_irqs_save();
        let old_handler =
            core::mem::replace(&mut TRAP_SCRATCH.as_mut().handler, captured_trap_handler);

        let mut to_ctx = TaskContext::new();
        to_ctx.set_program_counter(captured_trap_return as usize);
        to_ctx.set_stack_pointer(&raw mut *self as usize);
        to_ctx.set_interrupt_enabled(false);

        unsafe {
            TaskContext::switch(&mut TRAP_SCRATCH.as_mut().capturer_context, &mut to_ctx);
        }

        TRAP_SCRATCH.as_mut().handler = old_handler;
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
pub struct IrqState(Sstatus);

impl IrqState {
    #[inline]
    pub fn save() -> Self {
        IrqState(sstatus::read())
    }
}

impl IrqStateTrait for IrqState {
    fn restore(self) {
        let Self(state) = self;
        unsafe {
            sstatus::write(state);
        }
    }
}

#[inline]
pub fn disable_irqs() {
    unsafe {
        sstatus::clear_sie();
    }
}

#[inline]
pub fn enable_irqs() {
    unsafe {
        sstatus::set_sie();
    }
}

#[inline]
pub fn disable_irqs_save() -> IrqState {
    let state = IrqState::save();
    disable_irqs();

    state
}
