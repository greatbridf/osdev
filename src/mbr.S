.section .mbr
.code16

move_mbr:
    xor %ax, %ax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %ss

    # build a temporary stack
    xor %esp, %esp
    mov $0x0e00, %sp

    mov $128, %cx # 512 bytes
    mov $0x7c00, %si
    mov $0x0e00, %di
    rep movsl

    lgdt .Learly_gdt_descriptor

    mov $.Lread_data_packet, %si
    mov $0x42, %ah
    mov $0x80, %dl
    int $0x13
    jc .Lhalt

    # get memory size info and storage it
    mov $0xe801, %ax
    int $0x15
    jc .Lhalt

    cmp $0x86, %ah # unsupported function
    je .Lhalt
    cmp $0x80, %ah # invalid command
    je .Lhalt

    jcxz .Lax
    mov %cx, %ax
    mov %dx, %bx

.Lax:
    sub $1024, %esp
    movzw %ax, %eax
    mov %eax, 8(%esp)  # 1k blocks
    movzw %bx, %ebx
    mov %ebx, 12(%esp) # 64k blocks

    # save the destination address to es:di
    mov %sp, %di
    add $16, %di # buffer is 1024 - 16 bytes

    # set default entry size
    movl $20, 4(%esp)

    # clear %ebx, len
    xor %ebx, %ebx
    mov %ebx, (%esp)

.Le820_read_mem_map:
    # set the magic number to edx
    mov $0x534D4150, %edx

    # set function number to eax
    mov $0xe820, %eax

    # set default entry size
    mov $24, %ecx

    int $0x15

    incl (%esp)
    add $24, %edi

    jc .Lsave_mem_fin
    cmp $0, %ebx
    jz .Lsave_mem_fin

    cmp $24, %ecx
    cmovnz 4(%esp), %ecx
    mov %ecx, 4(%esp)

    jmp .Le820_read_mem_map

.Lsave_mem_fin:
    cli
    lidt .Lnull_idt_descriptor

    mov %cr0, %eax
    or $1, %eax
    mov %eax, %cr0

    ljmp $0x08, $start_32bit

.Lhalt:
    hlt
    jmp .Lhalt

.align 16
.Learly_gdt:
    .8byte 0x0                # null selector
    .8byte 0x00cf9a000000ffff # code selector
    .8byte 0x00cf92000000ffff # data selector
    .8byte 0x000f9a000000ffff # 16 bit code selector
    .8byte 0x000f92000000ffff # 16 bit data selector

# null IDT descriptor
# so that exceptions will cause the system to reset
.align 4
.Lnull_idt_descriptor:
    .word 0 # size
    .long 0 # base

.align 4
.Learly_gdt_descriptor:
    .word 0x27 # size
    .long .Learly_gdt  # address

.align 16
.Lread_data_packet:
    .long  0x00080010 # .stage1 takes up 4K, or 8 sectors
    .long  0x00001000 # read to 0000:1000
    .8byte 1          # read from LBA 1
