.text

.globl asm_outb
.type  asm_outb @function
asm_outb:
    push %rax
    push %rdx
    mov 12(%esp), %dx
    mov 16(%esp), %al
    outb %al, %dx
    pop %rdx
    pop %rax
    ret

.globl asm_inb
.type  asm_inb @function
asm_inb:
    push %rdx
    mov 8(%esp), %dx
    inb %dx, %al
    pop %rdx
    ret

.globl asm_hlt
.type  asm_hlt @function
asm_hlt:
    hlt
    ret

.globl asm_cli
.type  asm_cli @function
asm_cli:
    cli
    ret

.globl asm_sti
.type  asm_sti @function
asm_sti:
    sti
    ret
