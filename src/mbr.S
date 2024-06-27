.section .mbr
.code16

move_mbr:
    xor %ax, %ax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %ss

    # build a temporary stack
    mov $0x0e00, %esp
    mov %esp, %ebp

    mov $128, %cx # 512 / 4
    mov $0x7c00, %si
    mov $0x0e00, %di
    rep movsl

    ljmp $0x00, $mbr_start

# %eax: lba lower 4bytes
# %edx: destination address
read_disk:
	push %eax
	push %edx
	push %ecx

	mov %eax, read_data_lba
	shr $4, %edx
	mov %dx, read_data_segment

    mov $read_data_pack, %si
    mov $0x42, %ah
    mov $0x80, %dl
    int $0x13
    jc halt

	pop %ecx
	pop %edx
	pop %eax
	ret

mbr_start:
    # clear screen
    mov $0x00, %ah
    mov $0x03, %al
    int $0x10

    # read kernel image: 32K * 15 = 480K
	xor %eax, %eax
	inc %eax # %eax = 1
	mov %eax, %edx
	shl $12, %edx # %edx = 0x1000

	mov $15, %ecx
_loop_read_kernel:
	call read_disk
	add $64, %eax # %eax += 64

	shr $12, %edx
	add $8, %edx
	shl $12, %edx # %edx += 32K

	loop _loop_read_kernel

    # get memory size info and storage it
    xor %ecx, %ecx
    xor %edx, %edx
	xor %eax, %eax
    mov $0xe801, %ax

    int $0x15
    jc halt

    cmp $0x86, %ah # unsupported function
    je halt
    cmp $0x80, %ah # invalid command
    je halt

    jcxz _get_memory_size_use_ax
    mov %cx, %ax
    mov %dx, %bx

_get_memory_size_use_ax:
    sub $1024, %esp
    movzw %ax, %eax
    mov %eax, 8(%esp)  # 1k blocks
    movzw %bx, %ebx
    mov %ebx, 12(%esp) # 64k blocks

    # save the destination address to es:di
    lea 16(%esp), %di # buffer is 1024 - 16 bytes

    # clear %ebx, len
    xor %ebx, %ebx
    mov %ebx, (%esp)

    # set default entry size
    movl $20, 4(%esp)

_e820_mem_map_load_loop:
    # set the magic number to edx
    mov $0x534D4150, %edx

    # set function number to eax
    mov $0xe820, %eax

    # set default entry size
    mov $24, %ecx

    int $0x15

    incl (%esp)
    add $24, %edi

    jc _e820_mem_map_load_fin
    cmp $0, %ebx
    jz _e820_mem_map_load_fin

    cmp $24, %ecx
    cmovnz 4(%esp), %ecx
    mov %ecx, 4(%esp)

    jmp _e820_mem_map_load_loop

_e820_mem_map_load_fin:
    # load GDT and IDT
    cli
    lidt null_idt_descriptor
    lgdt _32bit_gdt_descriptor

    # enable protection enable (PE) bit
    mov %cr0, %eax
    or $1, %eax
    mov %eax, %cr0

    ljmp $0x08, $start_32bit

halt:
    hlt
    jmp halt

.align 16
read_data_pack:
    .byte 0x10, 0
read_data_count:
    .word 64     # sector count (read 32k)
read_data_offset:
    .word 0x0000 # offset address
read_data_segment:
    .word 0x0100 # segment address
read_data_lba:
    .long 1      # lower 4 bytes of the LBA to read
    .long 0      # higher 2 bytes of the LBA to read

# null IDT descriptor
# so that exceptions will cause the system to reset
.align 4
null_idt_descriptor:
    .word 0 # size
    .long 0 # base

.align 4
_32bit_gdt_descriptor:
    .word (3 * 8) - 1 # size
    .long _32bit_gdt  # address

.align 16
_32bit_gdt:
    .8byte 0x0                # null selector
    .8byte 0x00cf9a000000ffff # code selector
    .8byte 0x00cf92000000ffff # data selector
