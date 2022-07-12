.code32

.text

.global asm_switch_pd
.type   asm_switch_pd @function
asm_switch_pd:
    movl 4(%esp), %eax
    movl %eax, %cr3
    ret

.global asm_enable_paging
.type   asm_enable_paging @function
asm_enable_paging:
    cli
    // page directory address
    movl 4(%esp), %eax
    movl %eax, %cr3

    movl %cr0, %eax
    // SET PE, WP, PG
    orl $0x80010001, %eax
    movl %eax, %cr0

    ret

.global current_pd
.type   current_pd @function
current_pd:
    movl %cr3, %eax
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
_asm_load_gdt_fin_ret:
    ret

.global asm_load_tr
.type   asm_load_tr @function
asm_load_tr:
    cli
    movl 4(%esp), %eax
    orl $0, %eax
    ltr %ax
    ret

.globl go_user_space
.type  go_user_space @function
go_user_space:
    movl $((4 * 8) | 3), %eax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %fs
    movw %ax, %gs

    movl 4(%esp), %ebx
    pushl $((4 * 8) | 3)
    pushl $0x40100000
    pushf
    # allow interrupts in user mode
    movl (%esp), %eax
    orl $0x200, %eax
    movl %eax, (%esp)
    pushl $((3 * 8) | 3)
    pushl %ebx

    iret
