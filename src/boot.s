.section .stage1
.code32
.globl start_32bit
start_32bit:
    mov $0x10, %ax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %fs
    mov %ax, %gs
    mov %ax, %ss

    cld
    xor %eax, %eax

    # clear paging structures
    mov $0x100000, %edi
    mov %edi, %ecx
    shr $2, %ecx # %ecx /= 4
    rep stosl

    # set P, RW, G
    mov $0x00000103, %ebx
	xor %edx, %edx
    mov $0x00101000, %esi

    # PML4E 0x000
    # we need the first 1GB identically mapped
    # so that we won't trigger a triple fault after
    # enabling paging
	lea -0x1000(%esi), %edi # %edi = 0x100000
    call fill_pxe

    # PML4E 0xff0
	mov $0x80000000, %edx
	lea 0xff0(%edi), %edi
	call fill_pxe
	xor %edx, %edx

    # setup PDPT for physical memory mapping
    mov %esi, %edi

    # set PS
    or $0x00000080, %ebx
    mov $256, %ecx
    xor %esi, %esi
_fill_loop1:
    call fill_pxe
    lea 8(%edi), %edi
    add $0x40000000, %esi # 1GB
    adc $0, %edx
    loop _fill_loop1

	mov $0x80000000, %edx

    # set PCD, PWT
    or $0x00000018, %ebx
    mov $256, %ecx
    xor %esi, %esi
_fill_loop2:
    call fill_pxe
    lea 8(%edi), %edi
    add $0x40000000, %esi # 1GB
    adc $0, %edx
    loop _fill_loop2

	xor %edx, %edx

    # PML4E 0xff8
    mov %edi, %esi # 0x102000
    mov $0x100ff8, %edi
    # clear PCD, PWT, PS
    and $(~0x00000098), %ebx
    call fill_pxe

    # PDPTE 0xff8
    lea 0xff8(%esi), %edi  # 0x102ff8
    lea 0x1000(%esi), %esi # 0x103000
    call fill_pxe

    # PDE 0xff0
    lea 0xff0(%esi), %edi  # 0x103ff0
    lea 0x1000(%esi), %esi # 0x104000
    call fill_pxe

    # fill PT (kernel image)
    mov %esi, %edi # 0x104000
    mov $0x2000, %esi

.extern KERNEL_PAGES
    mov $KIMAGE_PAGES, %ecx

_fill_loop3:
    call fill_pxe
    lea 8(%edi), %edi
	lea 0x1000(%esi), %esi
    loop _fill_loop3

    # set msr
    mov $0xc0000080, %ecx
    rdmsr
    or $0x900, %eax # set LME, NXE
    wrmsr

    # set cr4
    mov %cr4, %eax
    or $0xa0, %eax # set PAE, PGE
    mov %eax, %cr4

    # load new page table
	xor %eax, %eax
	inc %eax
	shl $20, %eax # %eax = 0x100000
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

    ljmp $0x08, $_64bit_entry

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
_64bit_entry:
	jmp start_64bit

.section .text.kinit
start_64bit:
    # set stack pointer and clear stack bottom
	movzw %sp, %rdi
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

_64bit_hlt:
	cli
	hlt
	jmp _64bit_hlt
