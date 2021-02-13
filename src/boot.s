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

# perform a temporary stack
    movw $(stack_base-_start), %ax
    movw %ax, %bp
    movw %ax, %sp

    call read_data

die:
    hlt
    jmp die

read_data:
    movw $(read_data_pack-_start), %si
    mov $0x42, %ah
    mov $0x80, %dl
    int $0x13
    ret

string_hello:
.string "Hello World!"

read_data_pack:
    .byte 0x10, 0
    .word 2      # block count
    .word 0x0000 # offset address
    .word 0x0050 # segment address
    .long  0     # LBA to read

stack_edge:
.space 128
stack_base:

.space 510 - (.-_start)
.word 0xaa55
