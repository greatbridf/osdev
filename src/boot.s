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
xorw %sp, %sp

// print hello world
mov $(string_hello-_start), %ax
mov %ax, %bp
movw $0x1301, %ax
movw $0x000f, %bx
movw $12, %cx
movw $0, %dx
int $0x10

die:
hlt
jmp die

string_hello:
.string "Hello World!"

.space 510 - (.-_start)
.word 0xaa55
