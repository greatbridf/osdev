// some old code

.altmacro
.macro SAVE_GP n
    sd x\n, \n*8(sp)
.endm
.macro LOAD_GP n
    ld x\n, \n*8(sp)
.endm

.section .text
    .globl trap_from_user
    .globl return_to_user
    .align 2

_raw_trap_entry:
    # swap sp and sscratch(previously stored user TrapContext's address in return_to_user)
    csrrw sp, sscratch, sp

    sd x1, 1*8(sp)
    .set n, 3
    .rept 29
        SAVE_GP %n
        .set n, n+1
    .endr

    csrr t0, sstatus
    csrr t1, sepc
    csrr t2, scause
    csrr t3, stval
    sd t0, 32*8(sp)     # save sstatus into the TrapContext
    sd t1, 33*8(sp)     # save sepc into the TrapContext
    sd t2, 34*8(sp)     # save scause into the TrapContext
    sd t3, 35*8(sp)     # save stval into the TrapContext

    csrr t0, sscratch
    sd t0, 2*8(sp)      # save user stack pointer into the TrapContext

    ld ra, 37*8(sp)
    ld s0, 38*8(sp)
    ld s1, 39*8(sp)
    ld s2, 40*8(sp)
    ld s3, 41*8(sp)
    ld s4, 42*8(sp)
    ld s5, 43*8(sp)
    ld s6, 44*8(sp)
    ld s7, 45*8(sp)
    ld s8, 46*8(sp)
    ld s9, 47*8(sp)
    ld s10, 48*8(sp)
    ld s11, 49*8(sp)

    ld fp, 50*8(sp)
    ld tp, 51*8(sp)

    ld sp, 36*8(sp)
    ret

# a0: pointer to TrapContext in user space (constant)
_raw_trap_return:
    # sscratch store the TrapContext's address
    csrw sscratch, a0

    # offset in TrapContext's order
    sd sp, 36*8(a0)
    sd ra, 37*8(a0)
    sd s0, 38*8(a0)
    sd s1, 39*8(a0) 
    sd s2, 40*8(a0)
    sd s3, 41*8(a0)
    sd s4, 42*8(a0)
    sd s5, 43*8(a0)
    sd s6, 44*8(a0)
    sd s7, 45*8(a0)
    sd s8, 46*8(a0)
    sd s9, 47*8(a0)
    sd s10, 48*8(a0)
    sd s11, 49*8(a0)
    sd fp, 50*8(a0)
    sd tp, 51*8(a0)

    mv sp, a0
    # now sp points to TrapContext in kernel space

    # restore sstatus and sepc
    ld t0, 32*8(sp)
    ld t1, 33*8(sp)
    ld t2, 34*8(sp)
    ld t3, 35*8(sp)
    csrw sstatus, t0
    csrw sepc, t1
    csrw scause, t2
    csrw stval, t3

    # save x* expect x0 and sp
    ld x1, 1*8(sp)
    .set n, 3
    .rept 29
        LOAD_GP %n
        .set n, n+1
    .endr
    ld sp, 2*8(sp)

    sret
