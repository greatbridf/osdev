mod trap_context;

use super::context::TaskContext;
use core::arch::{asm, global_asm, naked_asm};
use core::cell::UnsafeCell;
use core::mem::{offset_of, size_of};
use core::num::NonZero;
use core::ptr::NonNull;
use eonix_hal_traits::{
    context::RawTaskContext,
    trap::{IrqState as IrqStateTrait, TrapReturn},
};
use loongArch64::register::crmd::{self, Crmd};
use loongArch64::register::ecfg;
use loongArch64::register::eentry::{self, Eentry};

pub use trap_context::*;

pub const CSR_KERNEL_TP: usize = 0x30;
const CSR_CAPTURED_TRAP_CONTEXT_ADDR: usize = 0x31;
const CSR_CAPTURER_TASK_CONTEXT_ADDR: usize = 0x32;
const CSR_T0: usize = 0x33;
const CSR_T1: usize = 0x34;

#[unsafe(naked)]
unsafe extern "C" fn _raw_trap_entry() -> ! {
    naked_asm!(
        // Page alignment is required for trap entry points
        ".align 12",
        "csrwr  $t0,  {CSR_T0}",
        "csrwr  $t1,  {CSR_T1}",
        "csrrd  $t0,  {CSR_CAPTURED_TRAP_CONTEXT_ADDR}",
        "move   $t1,  $sp",
        "bnez   $t0,  2f",
        // We came here from normal execution
        "li.d   $t0, -16",
        "and    $t0,  $t0, $sp",
        "addi.d $t0,  $t0, -{trap_context_size}",
        "move   $sp,  $t0",
        // t0: &mut TrapContext
        "2:",
        "st.d   $ra,  $t0, {ra}",
        "st.d   $tp,  $t0, {tp}",
        "st.d   $t1,  $t0, {sp}", // $sp is saved in $t1
        "csrrd  $ra,  {CSR_T0}", // Put old $t0 into $ra
        "csrrd  $tp,  {CSR_T1}", // Put old $t1 into $tp
        "st.d   $a0,  $t0, {a0}",
        "st.d   $a1,  $t0, {a1}",
        "st.d   $a2,  $t0, {a2}",
        "st.d   $a3,  $t0, {a3}",
        "st.d   $a4,  $t0, {a4}",
        "st.d   $a5,  $t0, {a5}",
        "st.d   $a6,  $t0, {a6}",
        "st.d   $a7,  $t0, {a7}",
        "st.d   $ra,  $t0, {t0}", // $t0 is saved in $ra
        "st.d   $tp,  $t0, {t1}", // $t1 is saved in $tp
        "st.d   $t2,  $t0, {t2}",
        "st.d   $t3,  $t0, {t3}",
        "st.d   $t4,  $t0, {t4}",
        "st.d   $t5,  $t0, {t5}",
        "st.d   $t6,  $t0, {t6}",
        "st.d   $t7,  $t0, {t7}",
        "st.d   $t8,  $t0, {t8}",
        "st.d   $r21, $t0, {u0}",
        "st.d   $fp,  $t0, {fp}",
        "csrrd  $tp,  {CSR_KERNEL_TP}",
        "csrrd  $t1,  {CSR_ESTAT}",
        "csrrd  $t2,  {CSR_PRMD}",
        "csrrd  $ra,  {CSR_ERA}",
        "csrrd  $a1,  {CSR_CAPTURER_TASK_CONTEXT_ADDR}",
        "st.d   $s0,  $t0, {s0}",
        "st.d   $s1,  $t0, {s1}",
        "st.d   $s2,  $t0, {s2}",
        "st.d   $s3,  $t0, {s3}",
        "st.d   $s4,  $t0, {s4}",
        "st.d   $s5,  $t0, {s5}",
        "st.d   $s6,  $t0, {s6}",
        "st.d   $s7,  $t0, {s7}",
        "st.d   $s8,  $t0, {s8}",
        "st.d   $t1,  $t0, {estat}",
        "st.d   $t2,  $t0, {prmd}",
        "st.d   $ra,  $t0, {era}",
        "bnez   $a1,  {captured_trap_handler}",
        "b      {default_trap_handler}",
        CSR_KERNEL_TP = const CSR_KERNEL_TP,
        CSR_CAPTURED_TRAP_CONTEXT_ADDR = const CSR_CAPTURED_TRAP_CONTEXT_ADDR,
        CSR_CAPTURER_TASK_CONTEXT_ADDR = const CSR_CAPTURER_TASK_CONTEXT_ADDR,
        CSR_T0 = const CSR_T0,
        CSR_T1 = const CSR_T1,
        CSR_ESTAT = const 0x5,
        CSR_PRMD = const 0x1,
        CSR_ERA = const 0x6,
        trap_context_size = const size_of::<TrapContext>(),
        ra = const Registers::OFFSET_RA,
        tp = const Registers::OFFSET_TP,
        sp = const Registers::OFFSET_SP,
        a0 = const Registers::OFFSET_A0,
        a1 = const Registers::OFFSET_A1,
        a2 = const Registers::OFFSET_A2,
        a3 = const Registers::OFFSET_A3,
        a4 = const Registers::OFFSET_A4,
        a5 = const Registers::OFFSET_A5,
        a6 = const Registers::OFFSET_A6,
        a7 = const Registers::OFFSET_A7,
        t0 = const Registers::OFFSET_T0,
        t1 = const Registers::OFFSET_T1,
        t2 = const Registers::OFFSET_T2,
        t3 = const Registers::OFFSET_T3,
        t4 = const Registers::OFFSET_T4,
        t5 = const Registers::OFFSET_T5,
        t6 = const Registers::OFFSET_T6,
        t7 = const Registers::OFFSET_T7,
        t8 = const Registers::OFFSET_T8,
        u0 = const Registers::OFFSET_U0,
        fp = const Registers::OFFSET_FP,
        s0 = const Registers::OFFSET_S0,
        s1 = const Registers::OFFSET_S1,
        s2 = const Registers::OFFSET_S2,
        s3 = const Registers::OFFSET_S3,
        s4 = const Registers::OFFSET_S4,
        s5 = const Registers::OFFSET_S5,
        s6 = const Registers::OFFSET_S6,
        s7 = const Registers::OFFSET_S7,
        s8 = const Registers::OFFSET_S8,
        estat = const TrapContext::OFFSET_ESTAT,
        prmd = const TrapContext::OFFSET_PRMD,
        era = const TrapContext::OFFSET_ERA,
        captured_trap_handler = sym captured_trap_handler,
        default_trap_handler = sym default_trap_handler,
    );
}

#[unsafe(naked)]
unsafe extern "C" fn _raw_trap_return(ctx: &mut TrapContext) -> ! {
    naked_asm!(
        "ld.d  $ra,  $s8, {ra}",
        "ld.d  $tp,  $s8, {tp}",
        "ld.d  $sp,  $s8, {sp}",
        "ld.d  $a0,  $s8, {a0}",
        "ld.d  $a1,  $s8, {a1}",
        "ld.d  $a2,  $s8, {a2}",
        "ld.d  $a3,  $s8, {a3}",
        "ld.d  $a4,  $s8, {a4}",
        "ld.d  $a5,  $s8, {a5}",
        "ld.d  $a6,  $s8, {a6}",
        "ld.d  $a7,  $s8, {a7}",
        "ld.d  $t0,  $s8, {t0}",
        "ld.d  $t1,  $s8, {t1}",
        "ld.d  $t2,  $s8, {t2}",
        "ld.d  $t3,  $s8, {t3}",
        "ld.d  $t4,  $s8, {t4}",
        "ld.d  $t5,  $s8, {t5}",
        "ld.d  $t6,  $s8, {t6}",
        "ld.d  $t7,  $s8, {t7}",
        "ld.d  $t8,  $s8, {t8}",
        "ld.d  $r21, $s8, {u0}",
        "ld.d  $fp,  $s8, {fp}",
        "ld.d  $s6,  $s8, {prmd}",
        "ld.d  $s7,  $s8, {era}",
        "ld.d  $s0,  $s8, {s0}",
        "ld.d  $s1,  $s8, {s1}",
        "ld.d  $s2,  $s8, {s2}",
        "ld.d  $s3,  $s8, {s3}",
        "ld.d  $s4,  $s8, {s4}",
        "ld.d  $s5,  $s8, {s5}",
        "csrwr $s6,  {CSR_PRMD}",
        "csrwr $s7,  {CSR_ERA}",
        "ld.d  $s6,  $s8, {s6}",
        "ld.d  $s7,  $s8, {s7}",
        "ld.d  $s8,  $s8, {s8}",
        "ertn",
        CSR_PRMD = const 0x1,
        CSR_ERA = const 0x6,
        ra = const Registers::OFFSET_RA,
        tp = const Registers::OFFSET_TP,
        sp = const Registers::OFFSET_SP,
        a0 = const Registers::OFFSET_A0,
        a1 = const Registers::OFFSET_A1,
        a2 = const Registers::OFFSET_A2,
        a3 = const Registers::OFFSET_A3,
        a4 = const Registers::OFFSET_A4,
        a5 = const Registers::OFFSET_A5,
        a6 = const Registers::OFFSET_A6,
        a7 = const Registers::OFFSET_A7,
        t0 = const Registers::OFFSET_T0,
        t1 = const Registers::OFFSET_T1,
        t2 = const Registers::OFFSET_T2,
        t3 = const Registers::OFFSET_T3,
        t4 = const Registers::OFFSET_T4,
        t5 = const Registers::OFFSET_T5,
        t6 = const Registers::OFFSET_T6,
        t7 = const Registers::OFFSET_T7,
        t8 = const Registers::OFFSET_T8,
        u0 = const Registers::OFFSET_U0,
        fp = const Registers::OFFSET_FP,
        s0 = const Registers::OFFSET_S0,
        s1 = const Registers::OFFSET_S1,
        s2 = const Registers::OFFSET_S2,
        s3 = const Registers::OFFSET_S3,
        s4 = const Registers::OFFSET_S4,
        s5 = const Registers::OFFSET_S5,
        s6 = const Registers::OFFSET_S6,
        s7 = const Registers::OFFSET_S7,
        s8 = const Registers::OFFSET_S8,
        prmd = const TrapContext::OFFSET_PRMD,
        era = const TrapContext::OFFSET_ERA,
    );
}

#[unsafe(naked)]
unsafe extern "C" fn default_trap_handler() {
    unsafe extern "C" {
        fn _default_trap_handler(trap_context: &mut TrapContext);
    }

    #[cfg(debug_assertions)]
    naked_asm!(
        ".cfi_startproc",
        ".cfi_signal_frame",
        "move $s8, $t0",
        "move $a0, $t0",
        "",
        ".cfi_register $ra, $s7",
        "move $s7, $ra",
        "",
        "bl   {default_handler}",
        "",
        "b    {trap_return}",
        "",
        ".cfi_endproc",
        default_handler = sym _default_trap_handler,
        trap_return = sym _raw_trap_return,
    );

    #[cfg(not(debug_assertions))]
    naked_asm!(
        "move $s8, $t0",
        "move $a0, $t0",
        "",
        "bl   {default_handler}",
        "b    {trap_return}",
        default_handler = sym _default_trap_handler,
        trap_return = sym _raw_trap_return,
    );
}

static DIRTY_TASK_CONTEXT: TaskContext = unsafe { core::mem::zeroed() };

#[unsafe(naked)]
unsafe extern "C" fn captured_trap_handler() {
    naked_asm!(
        "la.global $a0, {dirty_task_context}",
        "b         {switch}",
        dirty_task_context = sym DIRTY_TASK_CONTEXT,
        switch = sym TaskContext::switch,
    );
}

#[unsafe(naked)]
unsafe extern "C" fn captured_trap_return(trap_context: usize) -> ! {
    naked_asm!(
        "move $s8, $sp",
        "b    {raw_trap_return}",
        raw_trap_return = sym _raw_trap_return,
    );
}

impl TrapReturn for TrapContext {
    type TaskContext = TaskContext;

    unsafe fn trap_return(&mut self) {
        let irq_states = disable_irqs_save();

        let mut capturer_ctx = TaskContext::new();
        let mut to_ctx = TaskContext::new();
        to_ctx.set_program_counter(captured_trap_return as usize);
        to_ctx.set_stack_pointer(&raw mut *self as usize);
        to_ctx.set_interrupt_enabled(false);

        unsafe {
            asm!(
                "csrwr {captured_trap_context}, {CSR_CAPTURED_TRAP_CONTEXT_ADDR}",
                "csrwr {capturer_task_context}, {CSR_CAPTURER_TASK_CONTEXT_ADDR}",
                captured_trap_context = inout(reg) &raw mut *self => _,
                capturer_task_context = inout(reg) &raw mut capturer_ctx => _,
                CSR_CAPTURED_TRAP_CONTEXT_ADDR = const CSR_CAPTURED_TRAP_CONTEXT_ADDR,
                CSR_CAPTURER_TASK_CONTEXT_ADDR = const CSR_CAPTURER_TASK_CONTEXT_ADDR,
                options(nomem, nostack, preserves_flags),
            );

            TaskContext::switch(&mut capturer_ctx, &mut to_ctx);

            asm!(
                "csrwr $zero, {CSR_CAPTURED_TRAP_CONTEXT_ADDR}",
                "csrwr $zero, {CSR_CAPTURER_TASK_CONTEXT_ADDR}",
                CSR_CAPTURED_TRAP_CONTEXT_ADDR = const CSR_CAPTURED_TRAP_CONTEXT_ADDR,
                CSR_CAPTURER_TASK_CONTEXT_ADDR = const CSR_CAPTURER_TASK_CONTEXT_ADDR,
                options(nomem, nostack, preserves_flags),
            )
        }

        irq_states.restore();
    }
}

fn setup_trap_handler(trap_entry_addr: usize) {
    ecfg::set_vs(0);
    eentry::set_eentry(trap_entry_addr);
}

pub fn setup_trap() {
    setup_trap_handler(_raw_trap_entry as usize);
}

#[derive(Debug, Clone, Copy)]
pub struct IrqState(bool);

impl IrqState {
    #[inline]
    pub fn save() -> Self {
        IrqState(crmd::read().ie())
    }
}

impl IrqStateTrait for IrqState {
    fn restore(self) {
        let Self(state) = self;
        crmd::set_ie(state)
    }
}

#[inline]
pub fn disable_irqs() {
    crmd::set_ie(false);
}

#[inline]
pub fn enable_irqs() {
    crmd::set_ie(true);
}

#[inline]
pub fn disable_irqs_save() -> IrqState {
    let state = IrqState::save();
    disable_irqs();

    state
}
