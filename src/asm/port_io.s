.code32

.text

.globl asm_outb
.type  asm_outb @function
asm_outb:
    pushl %eax
    pushl %edx
    movw 12(%esp), %dx
    movb 16(%esp), %al
    outb %al, %dx
    popl %edx
    popl %eax
    ret

.globl asm_inb
.type  asm_inb @function
asm_inb:
    pushl %edx
    movw 8(%esp), %dx
    inb %dx, %al
    popl %edx
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

.globl asm_enable_sse
.type  asm_enable_sse @function
asm_enable_sse:
	movl %cr0, %eax
    andl $0xfffffff3, %eax
	orl $0b100010, %eax
	movl %eax, %cr0
	movl %cr4, %eax
	orl $0b11000000000, %eax
	movl %eax, %cr4
    fninit
	ret
