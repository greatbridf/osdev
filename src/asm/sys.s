.code32

.text

.global asm_enable_paging
.type   asm_enable_paging @function
asm_enable_paging:
    cli
    // page directory address
    movl 4(%esp), %eax
    movl %eax, %cr3

    movl %cr0, %eax
    orl $0x80000001, %eax
    movl %eax, %cr0

    ret

.global asm_load_gdt
.type   asm_load_gdt @function
asm_load_gdt:
    cli
    leal 6(%esp), %eax
    lgdt (%eax)
    ljmp $0x08, $_asm_load_gdt_fin
_asm_load_gdt_fin:
	movw 4(%esp), %ax
	cmpw $0, %ax
	je _asm_load_gdt_fin_ret
    sti
_asm_load_gdt_fin_ret:
    ret

.global asm_load_tr
.type   asm_load_tr @function
asm_load_tr:
    cli
    movl 4(%esp), %eax
    orl $0, %eax
    ltr %ax
    sti
    ret


# examples for going ring 3
_test_user_space_program:
    movl $0x1919810, %eax
    movl $0xc48c, %ecx
_reap:
    cmpl $1000, (%ecx)
    jl _reap
_fault:
    cli

go_user_space_example:
    movl $((4 * 8) | 3), %eax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %fs
    movw %ax, %gs

    movl %esp, %eax
    pushl $((4 * 8) | 3)
    pushl %eax
    pushf
    pushl $((3 * 8) | 3)
    pushl $_test_user_space_program

    iret
