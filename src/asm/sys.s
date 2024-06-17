.text

.global asm_switch_pd
.type   asm_switch_pd @function
asm_switch_pd:
    mov 8(%rsp), %rax
    shl $12, %rax
    mov %rax, %cr3
    ret

.global current_pd
.type   current_pd @function
current_pd:
    mov %cr3, %rax
    ret

.section .text.kinit

.global asm_enable_paging
.type   asm_enable_paging @function
asm_enable_paging:
    cli
    // page directory address
    mov 8(%rsp), %rax
    mov %rax, %cr3

    mov %cr0, %rax
    // SET PE, WP, PG
	mov $0x80010001, %rcx
	or %rcx, %rax
    mov %rax, %cr0

    ret

.global asm_load_gdt
.type   asm_load_gdt @function
asm_load_gdt:
    ret
# TODO: LONG MODE
#     cli
#     lea 14(%rsp), %rax
#     lgdt (%rax)
#     ljmp $0x08, $_asm_load_gdt_fin
# _asm_load_gdt_fin:
#     ret

.global asm_load_tr
.type   asm_load_tr @function
asm_load_tr:
    cli
    mov 8(%rsp), %rax
    ltr %ax
    ret
