    .section .text.entry
    .global _start
_start:
    la sp, stack_top

    la a0, BSS_START
    la a1, BSS_END
// clear bss section
.Lclear_bss_loop:
    bge a0, a1, .Lclear_bss_done
    sw x0, (a0)
    addi a0, a0, 4
    j .Lclear_bss_loop
.Lclear_bss_done:
    // jump to kernel entry point
    call start
.Lhang:
    wfi
    j .Lhang

    .section .bss.stack
    .globl stack_bottom
stack_bottom:
    .space 4096 * 16
    .globl stack_top
stack_top:
