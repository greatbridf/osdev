.section .text.bootsect
.code16

.globl mbr_start
mbr_start:
    movw %cs, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss

# perform a temporary stack
    movw $stack_base, %ax
    movw %ax, %bp
    movw %ax, %sp

# read the first 64k
    call read_data

# read the rest
    addw $(0x100 * 16), read_data_segment
    addl $(8 * 16), read_data_lba
    call read_data

# loader start
    jmp 0x7e00

read_data:
    movw $read_data_pack, %si
    mov $0x42, %ah
    mov $0x80, %dl
    int $0x13
    jc read_data_error
    ret

read_data_error:
    hlt
    jmp read_data_error

.align 4
read_data_pack:
    .byte 0x10, 0
read_data_count:
    .word 128    # sector count (read 64k)
read_data_offset:
    .word 0x0000 # offset address
read_data_segment:
    .word 0x07e0 # segment address
read_data_lba:
    .long 1      # lower 4 bytes of the LBA to read
    .long 0      # higher 2 bytes of the LBA to read

__mbr_code_border__:
    .long 0xffffffff

.align 16
stack_edge:
.space 128
stack_base:

. = 510
.byte 0x55, 0xaa
