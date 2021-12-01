.section .text.bootsect
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

    ljmp $0x0060, $(loader_start-loader_start)

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
    .word 32     # block count (read 16k)
    .word 0x0000 # offset address
    .word 0x0060 # segment address
    .long 1      # LBA to read

stack_edge:
.space 128
stack_base:

.section .text.loader

# loader

loader_start:
# set segment registers
    movw %cs, %ax
    movw %ax, %ds

_clear_screen:
    mov $0x00, %ah
    mov $0x03, %al
    int $0x10

# get memory size info and storage it
_get_memory_size:
    xorw %cx, %cx
    xorw %dx, %dx
    movw $0xe801, %ax

    int $0x15
    jc _get_memory_size_error

    cmpb $0x86, %ah # unsupported function
    je _get_memory_size_error
    cmpb $0x80, %ah # invalid command
    je _get_memory_size_error

    jcxz _get_memory_size_use_ax
    movw %cx, %ax
    movw %dx, %bx

_get_memory_size_use_ax:
    movl $(asm_mem_size_info-loader_start), %edx
    movw %ax, (%edx)
    addw $2, %dx
    movw %bx, (%edx)
    jmp _e820_mem_map_load

_get_memory_size_error:
    xchgw %bx, %bx
    jmp loader_halt

_e820_mem_map_load:
    addl $4, %esp
    movl $0, (%esp)

    # save the destination address to es:di
    movw %cs, %ax
    movw %ax, %es

    movl $(asm_e820_mem_map-loader_start), %edi

    # clear ebx
    xorl %ebx, %ebx

    # set the magic number to edx
    movl $0x534D4150, %edx

_e820_mem_map_load_loop:
    # set function number to eax
    movl $0xe820, %eax

    # set default entry size
    movl $24, %ecx

    int $0x15

    incl (%esp)
    addl %ecx, %edi

    jc _e820_mem_map_load_fin
    cmpl $0, %ebx
    jz _e820_mem_map_load_fin
    jmp _e820_mem_map_load_loop

_e820_mem_map_load_fin:
    movl (%esp), %eax
    movl $(asm_e820_mem_map_count-loader_start), %edi
    movl %eax, (%edi)

    movl $(asm_e820_mem_map_entry_size-loader_start), %edi
    movl %ecx, (%edi)

    jmp _load_gdt

_load_gdt:
    cli
    lgdt (asm_gdt_descriptor-loader_start)

# enable protection enable (PE) bit
    movl %cr0, %eax
    orl $1, %eax
    movl %eax, %cr0

    ljmp $0x08, $0x0600 + (start_32bit-loader_start)

.code32

start_32bit:
    movw $16, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %fs
    movw %ax, %gs
    movw %ax, %ss

# set up stack
# in order to align 16 byte
# set stack base address at
# 0x003ffff0
    movl $0x03fffff0, %ebp
    movl $0x03fffff0, %esp

    call kernel_main

loader_halt:
    hlt
    jmp loader_halt

asm_gdt_descriptor:
    .word (3 * 8) - 1 # size
    .long 0x0600+(asm_gdt_table-loader_start)  # address

.globl asm_gdt_descriptor
.type asm_gdt_descriptor @object
.size asm_gdt_descriptor, (.-asm_gdt_descriptor)

asm_gdt_table:
    .8byte 0         # null descriptor

    # code segment
    .word 0x3fff     # limit 0 :15
    .word 0x0000     # base  0 :15
    .byte 0x00       # base  16:23
    .byte 0x9a       # access
    .byte 0b11000000 # flag and limit 16:20
    .byte 0x00       # base 24:31

    # data segment
    .word 0x3fff     # limit 0 :15
    .word 0x0000     # base  0 :15
    .byte 0x00       # base  16:23
    .byte 0x92       # access
    .byte 0b11000000 # flag and limit 16:20
    .byte 0x00       # base 24:31

asm_mem_size_info:
    .word 0x12
    .word 0x34

.globl asm_mem_size_info
.type  asm_mem_size_info @object
.size  asm_mem_size_info, (.-asm_mem_size_info)

asm_e820_mem_map:
    .space 1024
.globl asm_e820_mem_map
.type  asm_e820_mem_map @object
.size  asm_e820_mem_map, (.-asm_e820_mem_map)

asm_e820_mem_map_count:
    .long 0
.globl asm_e820_mem_map_count
.type  asm_e820_mem_map_count @object

asm_e820_mem_map_entry_size:
    .long 0
.globl asm_e820_mem_map_entry_size
.type  asm_e820_mem_map_entry_size @object
