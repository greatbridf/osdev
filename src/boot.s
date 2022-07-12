.section .text.loader
.code16
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
    movl $asm_mem_size_info, %edx
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

    movl $asm_e820_mem_map, %edi

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
    movl $asm_e820_mem_map_count, %edi
    movl %eax, (%edi)

    movl $asm_e820_mem_map_entry_size, %edi
    movl %ecx, (%edi)

    jmp _load_gdt

_load_gdt:
    cli
    lgdt asm_gdt_descriptor

# enable protection enable (PE) bit
    movl %cr0, %eax
    orl $1, %eax
    movl %eax, %cr0

    ljmp $0x08, $start_32bit

.code32

start_32bit:
    movw $16, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %fs
    movw %ax, %gs
    movw %ax, %ss

# set up early stack at 0x001000000
    movl $0x01000000, %ebp
    movl $0x01000000, %esp

setup_early_kernel_page_table:
# set up early kernel page table

# the early kernel page directory is located at physical
# address 0x00000000, size 4k, and the empty page is at
# 0x5000-0x5fff, so we fill the first 6KiB
    movl $0x00000000, %eax
    movl $0x6000, %ecx
    call _fill_zero

# map the first 16MiB identically
# 0x0000-0x0fff: early kernel pd
# 0x1000-0x4fff: pde 0 - 4
    movl $0x00000000, %eax
    movl $0x00001003, %ebx
_fill_pde_loop:
    movl %ebx, (%eax)
    addl $4, %eax
    addl $0x1000, %ebx
    cmpl $0x5003, %ebx
    jne _fill_pde_loop

# then, create page tables
    movl $0x00000003, %eax
    movl $0x00001000, %ecx

_create_page_table_loop1:
    movl %eax, (%ecx)
    addl $4, %ecx
    addl $0x1000, %eax
    cmpl $0x4ffc, %ecx
    jle _create_page_table_loop1

load_early_kernel_page_table:
    movl $0x00000000, %eax
    movl %eax, %cr3

    movl %cr0, %eax
    // SET PE, WP, PG
    orl $0x80010001, %eax
    movl %eax, %cr0

    jmp start_move_kernel

# quick call
# %eax: address to fill
# %ecx: byte count to fill
_fill_zero:
    movl %ecx, -4(%esp)
    movl %eax, -8(%esp)

_fill_zero_loop:
    cmpl $0, %ecx
    jz _fill_zero_end
    subl $4, %ecx
    movl $0, (%eax)
    addl $4, %eax
    jmp _fill_zero_loop

_fill_zero_end:
    movl -8(%esp), %eax
    movl -4(%esp), %ecx
    ret

start_move_kernel:
# move the kernel to 0x100000
    movl $__loader_end, %eax
    movl $__real_kernel_start, %ebx

    movl $__p_kernel_text_and_data_size_addr, %ecx
    movl (%ecx), %ecx
    movl (%ecx), %ecx

_move_kernel:
    movl (%eax), %edx
    movl %edx, (%ebx)
    addl $4, %eax
    addl $4, %ebx
    subl $4, %ecx
    cmpl $0, %ecx
    jge _move_kernel

    call kernel_main

loader_halt:
    hlt
    jmp loader_halt

asm_gdt_descriptor:
    .word (5 * 8) - 1 # size
    .long asm_gdt_table  # address

.globl asm_gdt_descriptor
.type asm_gdt_descriptor @object
.size asm_gdt_descriptor, (.-asm_gdt_descriptor)

asm_gdt_table:
    .8byte 0         # null descriptor

    # kernel code segment
    .word 0xffff     # limit 0 :15
    .word 0x0000     # base  0 :15
    .byte 0x00       # base  16:23
    .byte 0x9a       # access
    .byte 0b11001111 # flag and limit 16:20
    .byte 0x00       # base 24:31

    # kernel data segment
    .word 0xffff     # limit 0 :15
    .word 0x0000     # base  0 :15
    .byte 0x00       # base  16:23
    .byte 0x92       # access
    .byte 0b11001111 # flag and limit 16:20
    .byte 0x00       # base 24:31

    # user code segment
    .word 0xffff     # limit 0 :15
    .word 0x0000     # base  0 :15
    .byte 0x00       # base  16:23
    .byte 0xfa       # access
    .byte 0b11001111 # flag and limit 16:20
    .byte 0x00       # base 24:31

    # user data segment
    .word 0xffff     # limit 0 :15
    .word 0x0000     # base  0 :15
    .byte 0x00       # base  16:23
    .byte 0xf2       # access
    .byte 0b11001111 # flag and limit 16:20
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
