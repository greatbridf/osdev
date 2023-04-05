.section .stage1
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
    jmp __stage1_halt

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
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %fs
    movw %ax, %gs
    movw %ax, %ss

    movl $0, %esp
    movl $0, %ebp

setup_early_kernel_page_table:
# memory map:
# 0x0000-0x1000: empty page
# 0x1000-0x2000: early kernel pd
# 0x2000-0x6000: 4 pts
# 0x6000-0x8000: early kernel stack
# so we fill the first 8KiB with zero
    movl $0x00000000, %eax
    movl $0x8000, %ecx

_fill_zero:
    cmpl $0, %ecx
    jz _fill_zero_end
    subl $4, %ecx
    movl $0, (%eax)
    addl $4, %eax
    jmp _fill_zero
_fill_zero_end:

# pt#0: 0x00000000 to 0x00400000
    movl $0x00001000, %eax
    movl $0x00002003, (%eax)
# pt#1: 0xc0000000 to 0xc0400000
    movl $0x00001c00, %eax
    movl $0x00003003, (%eax)
# pt#2: 0xff000000 to 0xff400000
    movl $0x00001ff0, %eax
    movl $0x00004003, (%eax)
# pt#3: 0xffc00000 to 0xffffffff
    movl $0x00001ffc, %eax
    movl $0x00005003, (%eax)

# map early kernel page directory to 0xff000000
    movl $0x00004000, %eax
    movl $0x00001003, (%eax)

# map kernel pt#2 to 0xff001000
    movl $0x00004004, %eax
    movl $0x00004003, (%eax)

# map __stage1_start ---- __kinit_end identically
    movl $__stage1_start, %ebx
    movl $__kinit_end, %ecx
    movl %ebx, %edx
    shrl $12, %edx
    andl $0x3ff, %edx


__map_stage1_kinit:
    leal 3(%ebx), %eax
    movl %eax, 0x00002000(, %edx, 4)
    addl $0x1000, %ebx
    incl %edx
    cmpl %ebx, %ecx
    jne __map_stage1_kinit

# map __text_start ---- __data_end to 0xc0000000
    movl %ecx, %ebx
    movl $__text_start, %edx
    shrl $12, %edx
    andl $0x3ff, %edx

    movl $__data_end, %ecx
    subl $__text_start, %ecx
    addl %ebx, %ecx

__map_kernel_space:
    leal 3(%ebx), %eax
    movl %eax, 0x00003000(, %edx, 4)
    addl $0x1000, %ebx
    incl %edx
    cmpl %ebx, %ecx
    jne __map_kernel_space

# map __data_end ---- __bss_end from 0x100000
    movl $0x100000, %ebx
    movl $__bss_end, %ecx
    subl $__data_end, %ecx
    addl %ebx, %ecx

__map_kernel_bss:
    leal 3(%ebx), %eax
    movl %eax, 0x00003000(, %edx, 4)
    addl $0x1000, %ebx
    incl %edx
    cmpl %ebx, %ecx
    jne __map_kernel_bss

# map kernel stack 0xffffe000-0xffffffff
    movl $0x6000, %ebx
    movl $0x8000, %ecx
    movl $0x0ffffe, %edx
    andl $0x3ff, %edx

__map_kernel_stack:
    leal 3(%ebx), %eax
    movl %eax, 0x00005000(, %edx, 4)
    addl $0x1000, %ebx
    incl %edx
    cmpl %ebx, %ecx
    jne __map_kernel_stack

load_early_kernel_page_table:
    movl $0x00001000, %eax
    movl %eax, %cr3

    movl %cr0, %eax
    // SET PE, WP, PG
    orl $0x80010001, %eax
    movl %eax, %cr0

# set stack pointer and clear stack bottom
    movl $0xfffffff0, %esp
    movl $0xfffffff0, %ebp

    movl $0x00, (%esp)
    movl $0x00, 4(%esp)
    movl $0x00, 8(%esp)
    movl $0x00, 12(%esp)

    call kernel_init

__stage1_halt:
    hlt
    jmp __stage1_halt

asm_gdt_descriptor:
    .word (5 * 8) - 1 # size
    .long asm_gdt_table  # address
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

.globl asm_mem_size_info
.type  asm_mem_size_info @object
.size  asm_mem_size_info, (.-asm_mem_size_info)
asm_mem_size_info:
    .word 0x12
    .word 0x34

.globl asm_e820_mem_map
.type  asm_e820_mem_map @object
.size  asm_e820_mem_map, (.-asm_e820_mem_map)
asm_e820_mem_map:
    .space 1024

.globl asm_e820_mem_map_count
.type  asm_e820_mem_map_count @object
asm_e820_mem_map_count:
    .long 0

.globl asm_e820_mem_map_entry_size
.type  asm_e820_mem_map_entry_size @object
asm_e820_mem_map_entry_size:
    .long 0
