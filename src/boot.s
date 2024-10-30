.section .stage1

#include <kernel/mem/paging_asm.h>

.code16

.align 4
.Lbios_idt_desc:
    .word 0x03ff     # size
    .long 0x00000000 # base

.align 4
.Lnull_idt_desc:
    .word 0 # size
    .long 0 # base

.Lhalt16:
    hlt
    jmp .Lhalt16

# scratch %eax
# return address should be of 2 bytes, and will be zero extended to 4 bytes
go_32bit:
    cli
    lidt .Lnull_idt_desc

    # set PE bit
    mov %cr0, %eax
    or $1, %eax
    mov %eax, %cr0

    ljmp $0x08, $.Lgo_32bit0

.Lgo_16bit0:
    mov $0x20, %ax
    mov %ax, %ds
    mov %ax, %ss

    lidt .Lbios_idt_desc

    mov %cr0, %eax
    and $0xfffffffe, %eax
    mov %eax, %cr0

    ljmp $0x00, $.Lgo_16bit1
.Lgo_16bit1:
    xor %ax, %ax
    mov %ax, %ds
    mov %ax, %ss
    mov %ax, %es

    sti

    pop %eax
    push %ax
    ret

.code32
# scratch %eax
# return address should be of 4 bytes, and extra 2 bytes will be popped from the stack
go_16bit:
    cli
    ljmp $0x18, $.Lgo_16bit0

.Lgo_32bit0:
    mov $0x10, %ax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %ss

    pop %ax
    movzw %ax, %eax
    push %eax
    ret

# build read disk packet on the stack and perform read operation
#
# read 32k to 0x2000 and then copy to destination
#
# %edi: lba start
# %esi: destination
.code32
read_disk:
    push %ebp
    mov %esp, %ebp

    lea -24(%esp), %esp

    mov $0x00400010, %eax # packet size 0, sector count 64
    mov %eax, (%esp)

    mov $0x02000000, %eax # destination address 0x0200:0x0000
    mov %eax, 4(%esp)

    mov %edi, 8(%esp)  # lba low 4bytes

    xor %eax, %eax
    mov %eax, 12(%esp) # lba high 2bytes

    mov %esi, %edi
    mov %esp, %esi # packet address

    call go_16bit
.code16
    mov $0x42, %ah
    mov $0x80, %dl
    int $0x13
    jc .Lhalt16

    call go_32bit
.code32
    # move data to destination
    mov $0x2000, %esi
    mov $8192, %ecx
    rep movsl

    mov %ebp, %esp
    pop %ebp
    ret

.globl start_32bit
start_32bit:
    mov $0x10, %ax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %ss

    # read kimage into memory
	lea -16(%esp), %esp
    mov $KIMAGE_32K_COUNT, %ecx
    movl $KERNEL_IMAGE_PADDR, 4(%esp) # destination address
	movl $9, (%esp) # LBA

.Lread_kimage:
	mov (%esp), %edi
	mov 4(%esp), %esi

	mov %ecx, %ebx
    call read_disk
	mov %ebx, %ecx

    addl $0x8000, 4(%esp)
	addl $64, (%esp)

    loop .Lread_kimage

	lea 16(%esp), %esp

    cld
    xor %eax, %eax

    # clear paging structures
    mov $0x2000, %edi
    mov $0x6000, %ecx
    shr $2, %ecx # %ecx /= 4
    rep stosl

    # set P, RW, G
    mov $(PA_P | PA_RW | PA_G), %ebx
    xor %edx, %edx
    mov $KERNEL_PDPT_PHYS_MAPPING, %esi

    # PML4E 0x000
    # we need the first 1GB identically mapped
    # so that we won't trigger a triple fault after
    # enabling paging
    mov $KERNEL_PML4, %edi
    call fill_pxe

    # PML4E 0xff0
    mov $(PA_NXE >> 32), %edx
    lea 0xff0(%edi), %edi
    call fill_pxe
    xor %edx, %edx

    # setup PDPT for physical memory mapping
    mov $KERNEL_PDPT_PHYS_MAPPING, %edi

    # set PS
    or $PA_PS, %ebx
    mov $256, %ecx
    xor %esi, %esi
.Lfill1:
    call fill_pxe
    lea 8(%edi), %edi
    add $0x40000000, %esi # 1GB
    adc $0, %edx
    loop .Lfill1

    mov $(PA_NXE >> 32), %edx

    # set PCD, PWT
    or $(PA_PCD | PA_PWT), %ebx
    mov $256, %ecx
    xor %esi, %esi
.Lfill2:
    call fill_pxe
    lea 8(%edi), %edi
    add $0x40000000, %esi # 1GB
    adc $0, %edx
    loop .Lfill2

    xor %edx, %edx

    # PML4E 0xff8
    mov $KERNEL_PDPT_KERNEL_SPACE, %esi
    mov $KERNEL_PML4, %edi
    lea 0xff8(%edi), %edi
    # clear PCD, PWT, PS
    and $(~(PA_PCD | PA_PWT | PA_PS)), %ebx
    call fill_pxe

    # PDPTE 0xff8
    mov $KERNEL_PDPT_KERNEL_SPACE, %edi
    lea 0xff8(%edi), %edi
    mov $KERNEL_PD_KIMAGE, %esi
    call fill_pxe

    # PDE 0xff0
    mov $KERNEL_PD_KIMAGE, %edi
    lea 0xff0(%edi), %edi
    mov $KERNEL_PT_KIMAGE, %esi # 0x104000
    call fill_pxe

    # fill PT (kernel image)
    mov $KERNEL_PT_KIMAGE, %edi
    mov $KERNEL_IMAGE_PADDR, %esi

    mov $KIMAGE_PAGES, %ecx

.Lfill3:
    call fill_pxe
    lea 8(%edi), %edi
    lea 0x1000(%esi), %esi
    loop .Lfill3

    # set msr
    mov $0xc0000080, %ecx
    rdmsr
    or $0x901, %eax # set LME, NXE, SCE
    wrmsr

    # set cr4
    mov %cr4, %eax
    or $0xa0, %eax # set PAE, PGE
    mov %eax, %cr4

    # load new page table
    mov $KERNEL_PML4, %eax
    mov %eax, %cr3

    mov %cr0, %eax
    // SET PE, WP, PG
    or $0x80010001, %eax
    mov %eax, %cr0

    # create gdt
    xor %eax, %eax # at 0x0000
    mov %eax, 0x00(%eax)
    mov %eax, 0x04(%eax) # null descriptor
    mov %eax, 0x08(%eax) # code segment lower
    mov %eax, 0x10(%eax) # data segment lower
    mov $0x00209a00, %ecx
    mov %ecx, 0x0c(%eax) # code segment higher
    mov $0x00009200, %ecx
    mov %ecx, 0x14(%eax) # data segment higher

    # gdt descriptor
    push %eax
    push %eax

    # pad with a word
    mov $0x00170000, %eax
    push %eax

    lgdt 2(%esp)
    add $12, %esp

    ljmp $0x08, $.L64bit_entry

# %ebx: attribute low
# %edx: attribute high
# %esi: page physical address
# %edi: page x entry address
fill_pxe:
    lea (%ebx, %esi, 1), %eax
    mov %eax, (%edi)
    mov %edx, 4(%edi)

    ret

.code64
.L64bit_entry:
    jmp start_64bit

.section .text.kinit
start_64bit:
    # set stack pointer and clear stack bottom
    mov %rsp, %rdi
    xor %rsp, %rsp
    inc %rsp
    neg %rsp
    shr $40, %rsp
    shl $40, %rsp

    add %rdi, %rsp
    mov %rsp, %rdi

    # make stack frame
    lea -16(%rsp), %rsp
    mov %rsp, %rbp

    xor %rax, %rax
    mov %rax, (%rsp)
    mov %rax, 8(%rsp)

    call kernel_init

.L64bit_hlt:
    cli
    hlt
    jmp .L64bit_hlt
