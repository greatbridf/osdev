use core::arch::naked_asm;
use core::mem::MaybeUninit;

use crate::arch::trap::Registers;
use crate::trap::TrapContext;

// If captured trap context is present, we use it directly.
// We need to restore the callee saved registers from that TrapContext.
#[unsafe(naked)]
pub(super) unsafe extern "C" fn _captured_trap_entry() -> ! {
    naked_asm!(
        "csrrw gp, sscratch, gp",

        "sd    a0, {a0}(gp)",
        "mv    a0, s0",
        "ld    s0, {s0}(gp)",
        "sd    a0, {s0}(gp)",

        "sd    a1, {a1}(gp)",
        "mv    a1, s1",
        "ld    s1, {s1}(gp)",
        "sd    a1, {s1}(gp)",

        "sd    a2, {a2}(gp)",
        "mv    a2, s2",
        "ld    s2, {s2}(gp)",
        "sd    a2, {s2}(gp)",

        "sd    a3, {a3}(gp)",
        "mv    a3, s3",
        "ld    s3, {s3}(gp)",
        "sd    a3, {s3}(gp)",

        "sd    a4, {a4}(gp)",
        "mv    a4, s4",
        "ld    s4, {s4}(gp)",
        "sd    a4, {s4}(gp)",

        "sd    a5, {a5}(gp)",
        "mv    a5, s5",
        "ld    s5, {s5}(gp)",
        "sd    a5, {s5}(gp)",

        "sd    a6, {a6}(gp)",
        "mv    a6, s6",
        "ld    s6, {s6}(gp)",
        "sd    a6, {s6}(gp)",

        "sd    a7, {a7}(gp)",
        "mv    a7, s7",
        "ld    s7, {s7}(gp)",
        "sd    a7, {s7}(gp)",

        "sd    t0, {t0}(gp)",
        "mv    t0, s8",
        "ld    s8, {s8}(gp)",
        "sd    t0, {s8}(gp)",

        "sd    t1, {t1}(gp)",
        "mv    t1, s9",
        "ld    s9, {s9}(gp)",
        "sd    t1, {s9}(gp)",

        "sd    t2, {t2}(gp)",
        "mv    t2, s10",
        "ld    s10, {s10}(gp)",
        "sd    t2, {s10}(gp)",

        "sd    t3, {t3}(gp)",
        "mv    t3, s11",
        "ld    s11, {s11}(gp)",
        "sd    t3, {s11}(gp)",

        "sd    t4, {t4}(gp)",
        "mv    t4, ra",
        "ld    ra, {ra}(gp)",
        "sd    t4, {ra}(gp)",

        "sd    t5, {t5}(gp)",
        "mv    t5, tp",
        "ld    tp, {tp}(gp)",
        "sd    t5, {tp}(gp)",

        "sd    t6, {t6}(gp)",
        "mv    t6, sp",
        "ld    sp, {sp}(gp)",
        "sd    t6, {sp}(gp)",

        "ld    t5, {sstatus}(gp)",

        "csrrw t0, sscratch, gp",
        "sd    t0, {gp}(gp)",

        "csrr  t1, sstatus",
        "sd    t1, {sstatus}(gp)",

        "csrr  t2, sepc",
        "sd    t2, {sepc}(gp)",

        "csrr  t3, scause",
        "sd    t3, {scause}(gp)",

        "csrr  t4, stval",
        "sd    t4, {stval}(gp)",

        "csrw  sstatus, t5",
        "ret",
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
        stval = const TrapContext::OFFSET_STVAL,
    );
}

#[unsafe(naked)]
pub(super) unsafe extern "C" fn _captured_trap_return(ctx: &mut TrapContext) {
    naked_asm!(
        "mv    t6, a0",

        "mv    a0, s0",
        "ld    s0, {s0}(t6)",
        "sd    a0, {s0}(t6)",

        "mv    a1, s1",
        "ld    s1, {s1}(t6)",
        "sd    a1, {s1}(t6)",

        "mv    a2, s2",
        "ld    s2, {s2}(t6)",
        "sd    a2, {s2}(t6)",

        "mv    a3, s3",
        "ld    s3, {s3}(t6)",
        "sd    a3, {s3}(t6)",

        "mv    a4, s4",
        "ld    s4, {s4}(t6)",
        "sd    a4, {s4}(t6)",

        "mv    a5, s5",
        "ld    s5, {s5}(t6)",
        "sd    a5, {s5}(t6)",

        "mv    a6, s6",
        "ld    s6, {s6}(t6)",
        "sd    a6, {s6}(t6)",

        "mv    a7, s7",
        "ld    s7, {s7}(t6)",
        "sd    a7, {s7}(t6)",

        "mv    t0, s8",
        "ld    s8, {s8}(t6)",
        "sd    t0, {s8}(t6)",

        "mv    t1, s9",
        "ld    s9, {s9}(t6)",
        "sd    t1, {s9}(t6)",

        "mv     t2, s10",
        "ld    s10, {s10}(t6)",
        "sd     t2, {s10}(t6)",

        "mv     t3, s11",
        "ld    s11, {s11}(t6)",
        "sd     t3, {s11}(t6)",

        "mv     t4, ra",
        "ld     ra, {ra}(t6)",
        "sd     t4, {ra}(t6)",

        "mv     t5, tp",
        "ld     tp, {tp}(t6)",
        "sd     t5, {tp}(t6)",

        "mv     a0, sp",
        "ld     sp, {sp}(t6)",
        "sd     a0, {sp}(t6)",

        "csrr   t4, sstatus",
        "ld     t5, {sstatus}(t6)",
        "sd     t4, {sstatus}(t6)",
        "ld     t4, {sepc}(t6)",

        "ld     gp, {gp}(t6)",
        "ld     a0, {a0}(t6)",
        "ld     a1, {a1}(t6)",
        "ld     a2, {a2}(t6)",
        "ld     a3, {a3}(t6)",
        "ld     a4, {a4}(t6)",
        "ld     a5, {a5}(t6)",
        "ld     a6, {a6}(t6)",
        "ld     a7, {a7}(t6)",

        "csrw   sstatus, t5",
        "csrw      sepc, t4",

        "ld     t0, {t0}(t6)",
        "ld     t1, {t1}(t6)",
        "ld     t2, {t2}(t6)",
        "ld     t3, {t3}(t6)",
        "ld     t4, {t4}(t6)",
        "ld     t5, {t5}(t6)",
        "ld     t6, {t6}(t6)",
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
