.text
.code16

.globl _start

_start:
    jmp $0x07c0, $(real_start-_start)

real_start:
    movw %cs, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movw $(stack_base-_start), %ax
    movw %ax, %bp
    movw %ax, %sp

    call print_hello

die:
    hlt
    jmp die

print_hello:
    push %bp
    mov %sp, %bp

    mov $(string_hello-_start), %ax
    push %bp
    mov %ax, %bp
    movw $0x1301, %ax
    movw $0x000f, %bx
    movw $12, %cx
    movw $0, %dx
    int $0x10
    pop %bp

    mov %bp, %sp
    pop %bp
    ret

string_hello:
.string "Hello World!"

stack_edge:
.space 128
stack_base:

.space 510 - (.-_start)
.word 0xaa55
