.text
.code16

# mbr

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

    ljmp $0x0050, $(loader_start-loader_start)

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
    .word 16     # block count (read 8k)
    .word 0x0000 # offset address
    .word 0x0050 # segment address
    .long 1      # LBA to read

stack_edge:
.space 128
stack_base:

.space 510 - (.-_start)
.word 0xaa55


# loader

loader_start:
# set segment registers
    movw %cs, %ax
    movw %ax, %ds

# load gdt
    cli
    lgdt (asm_gdt_descriptor-loader_start)

# enable protection enable (PE) bit
    movl %cr0, %eax
    orl $1, %eax
    movl %eax, %cr0

    ljmp $0x08, $0x0500 + (start_32bit-loader_start)

.code32

start_32bit:
    movw $16, %ax
    movw %ax, %ds
    movw %ax, %ss

# set up stack
    movl $0x003fffff, %ebp
    movl $0x003fffff, %esp

# breakpoint
#ifdef _DEBUG
    xchgw %bx, %bx
#endif

    call kernel_main

loader_halt:
    hlt
    jmp loader_halt

asm_gdt_descriptor:
    .word (3 * 8) - 1 # size
    .long 0x0500+(asm_gdt_table-loader_start)  # address

.globl asm_gdt_descriptor
.type asm_gdt_descriptor @object
.size asm_gdt_descriptor, (.-asm_gdt_descriptor)

asm_gdt_table:
    .8byte 0         # null descriptor

    # code segment
    .word 0x03ff     # limit 0 :15
    .word 0x0000     # base  0 :15
    .byte 0x00       # base  16:23
    .byte 0x9a       # access
    .byte 0b11000000 # flag and limit 16:20
    .byte 0x00       # base 24:31

    # data segment
    .word 0x03ff     # limit 0 :15
    .word 0x0000     # base  0 :15
    .byte 0x40       # base  16:23
    .byte 0x92       # access
    .byte 0b11000000 # flag and limit 16:20
    .byte 0x00       # base 24:31
