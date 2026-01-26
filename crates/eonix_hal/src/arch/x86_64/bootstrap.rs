pub(crate) mod init;

use core::arch::{global_asm, naked_asm};

use super::mm::{E820_MEM_MAP_DATA, PA_G, PA_NXE, PA_P, PA_PS, PA_RW};

const KERNEL_IMAGE_PADDR: usize = 0x200000;
const KERNEL_PML4: usize = 0x1000;
const KERNEL_PDPT_PHYS_MAPPING: usize = 0x2000;
const KERNEL_PDPT_KERNEL_SPACE: usize = 0x3000;
const KERNEL_PD_KIMAGE: usize = 0x4000;
const KERNEL_PT_KIMAGE: usize = 0x5000;

#[unsafe(link_section = ".low")]
static mut EARLY_GDT: [u64; 7] = [0; 7];

#[unsafe(no_mangle)]
#[unsafe(link_section = ".low")]
static mut EARLY_GDT_DESCRIPTOR: (u16, u32) = (0, 0);

#[unsafe(link_section = ".low")]
static mut BIOS_IDT_DESCRIPTOR: (u16, u32) = (0, 0);

unsafe extern "C" {
    fn KIMAGE_32K_COUNT();
    fn KIMAGE_PAGES();

    fn STAGE1_MAGIC();
    fn STAGE1_MAGIC_VALUE();

    fn start_32bit() -> !;
}

global_asm!(
    r#"
    .pushsection .mbr, "ax", @progbits
    .code16

    .globl move_mbr
    move_mbr:
        xor %ax, %ax
        mov %ax, %ds
        mov %ax, %es
        mov %ax, %ss

        # move the MBR to 0xe00
        mov $128, %cx # 512 bytes
        mov $0x7c00, %si
        mov $0x0e00, %di
        rep movsl

        ljmp $0x00, $2f

    2:
        # read the kernel stage1
        mov $.Lread_data_packet, %si
        mov $0x42, %ah
        mov $0x80, %dl
        int $0x13
        jc .Lhalt16

        # get memory size info and storage it
        mov $0xe801, %ax
        int $0x15
        jc .Lhalt16

        cmp $0x86, %ah # unsupported function
        je .Lhalt16
        cmp $0x80, %ah # invalid command
        je .Lhalt16

        jcxz 2f
        mov %cx, %ax
        mov %dx, %bx

    2:
        mov ${e820_data_addr}, %esp
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

    2:
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

        jmp 2b

    .Lsave_mem_fin:
        mov $0x3ff, %ax
        mov ${bios_idt_descriptor}, %di
        mov %ax, (%di)

        xor %eax, %eax
        mov %eax, 2(%di)

        lgdt .Learly_gdt_descriptor

        cli
        # IDT descriptor is 6 0's. borrow the null gdt entry
        lidt .Learly_gdt

        # enable protection mode
        mov %cr0, %eax
        or $1, %eax
        mov %eax, %cr0

        ljmp $0x08, ${start_32bit}

    .Lhalt16:
        hlt
        jmp .

    .align 16
    .Learly_gdt:
        .8byte 0x0                # null selector
        .8byte 0x00cf9a000000ffff # 32bit code selector
        .8byte 0x00cf92000000ffff # 32bit data selector

    .align 4
    .Learly_gdt_descriptor:
        .word 0x17 # size
        .long .Learly_gdt  # address

    .align 16
    .Lread_data_packet:
        .long  0x00070010 # .stage1 takes up 3.5K, or 7 sectors
        .long  0x00006000 # read to 0000:6000
        .8byte 1          # read from LBA 1
    .popsection
    "#,
    start_32bit = sym start_32bit,
    bios_idt_descriptor = sym BIOS_IDT_DESCRIPTOR,
    e820_data_addr = sym E820_MEM_MAP_DATA,
    options(att_syntax),
);

global_asm!(
    r#"
    .pushsection .stage1, "ax", @progbits
    .code16
    .Lhalt:
        hlt
        jmp .

    # scratch %eax
    # return address should be of 2 bytes, and will be zero extended to 4 bytes
    .Lgo_32bit:
        cli
        # borrow the null entry from the early gdt
        lidt {EARLY_GDT}

        # set PE bit
        mov %cr0, %eax
        or $1, %eax
        mov %eax, %cr0

        ljmp $0x18, $.Lgo_32bit0

    .Lgo_16bit0:
        mov $0x30, %ax
        mov %ax, %ds
        mov %ax, %es
        mov %ax, %ss

        lidt {BIOS_IDT_DESCRIPTOR}

        mov %cr0, %eax
        and $0xfffffffe, %eax
        mov %eax, %cr0

        ljmp $0x00, $2f

    2:
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
    .Lgo_16bit:
        cli
        ljmp $0x28, $.Lgo_16bit0

    .Lgo_32bit0:
        mov $0x20, %ax
        mov %ax, %ds
        mov %ax, %es
        mov %ax, %ss

        pop %ax
        movzw %ax, %eax
        push %eax
        ret

    # build read disk packet on the stack and perform read operation
    #
    # read 16k to 0x8000 and then copy to destination
    #
    # %edi: lba start
    # %esi: destination
    .code32
    read_disk:
        push %ebp
        mov %esp, %ebp

        lea -24(%esp), %esp

        mov $0x00200010, %eax # packet size 0, sector count 64
        mov %eax, (%esp)

        mov $0x08000000, %eax # destination address 0x0800:0x0000
        mov %eax, 4(%esp)

        mov %edi, 8(%esp)  # lba low 4bytes

        xor %eax, %eax
        mov %eax, 12(%esp) # lba high 2bytes

        mov %esi, %edi
        mov %esp, %esi # packet address

        call .Lgo_16bit
    .code16
        mov $0x42, %ah
        mov $0x80, %dl
        int $0x13
        jc .Lhalt

        call .Lgo_32bit
    .code32
        # move data to destination
        mov $0x8000, %esi
        mov $4096, %ecx
        rep movsl

        mov %ebp, %esp
        pop %ebp
        ret

    .align 8
    .Lgdt_data:
        .8byte 0x00209a0000000000 # 64bit code selector
        .8byte 0x0000920000000000 # 64bit data selector
        .8byte 0x00cf9a000000ffff # 32bit code selector
        .8byte 0x00cf92000000ffff # 32bit data selector
        .8byte 0x000f9a000000ffff # 16bit code selector
        .8byte 0x000f92000000ffff # 16bit data selector

    {start_32bit}:
        mov $0x10, %ax
        mov %ax, %ds
        mov %ax, %es
        mov %ax, %ss

        mov ${STAGE1_MAGIC}, %edi
        mov (%edi), %edi

        cmp ${STAGE1_MAGIC_VALUE}, %edi
        jne .Lhalt

        mov ${EARLY_GDT_DESCRIPTOR}, %edi
        mov $0x37, %ax
        mov %ax, (%edi)

        mov ${EARLY_GDT}, %eax
        mov %eax, 2(%edi)

        # fill in early kernel GDT
        xchg %eax, %edi
        xor %eax, %eax
        mov $2, %ecx

        # null segment
        rep stosl

        # other data
        mov $.Lgdt_data, %esi
        mov $12, %ecx

        rep movsl

        lgdt {EARLY_GDT_DESCRIPTOR}
        ljmp $0x18, $2f

    2:
        mov $0x20, %ax
        mov %ax, %ds
        mov %ax, %es
        mov %ax, %ss

        # temporary kernel stack
        mov $0x1000, %esp

        # read kimage into memory
        lea -16(%esp), %esp
        mov ${KIMAGE_32K_COUNT}, %ecx
        shl $1, %ecx
        movl ${KERNEL_IMAGE_PADDR}, 4(%esp) # destination address
        movl $8, (%esp) # LBA

    2:
        mov (%esp), %edi
        mov 4(%esp), %esi

        mov %ecx, %ebx
        call read_disk
        mov %ebx, %ecx

        addl $0x4000, 4(%esp)
        addl $32, (%esp)

        loop 2b

        lea 16(%esp), %esp

        cld
        xor %eax, %eax

        # clear paging structures
        mov $0x1000, %edi
        mov $0x5000, %ecx
        shr $2, %ecx # %ecx /= 4
        rep stosl

        # set P, RW, G
        mov $({PA_P} | {PA_RW} | {PA_G}), %ebx
        xor %edx, %edx
        mov ${KERNEL_PDPT_PHYS_MAPPING}, %esi

        # PML4E 0x000
        # we need the first 1GB identically mapped
        # so that we won't trigger a triple fault after
        # enabling paging
        mov ${KERNEL_PML4}, %edi
        call fill_pxe

        # PML4E 0xff0
        mov $({PA_NXE} >> 32), %edx
        lea 0xff0(%edi), %edi
        call fill_pxe
        xor %edx, %edx

        # setup PDPT for physical memory mapping
        mov ${KERNEL_PDPT_PHYS_MAPPING}, %edi

        # set PS
        or ${PA_PS}, %ebx
        mov $512, %ecx
        xor %esi, %esi
    2:
        call fill_pxe
        lea 8(%edi), %edi
        add $0x40000000, %esi # 1GB
        adc $0, %edx
        loop 2b

        xor %edx, %edx

        # PML4E 0xff8
        mov ${KERNEL_PDPT_KERNEL_SPACE}, %esi
        mov ${KERNEL_PML4}, %edi
        lea 0xff8(%edi), %edi
        # clear PS
        and $(~{PA_PS}), %ebx
        call fill_pxe

        # PDPTE 0xff8
        mov ${KERNEL_PDPT_KERNEL_SPACE}, %edi
        lea 0xff8(%edi), %edi
        mov ${KERNEL_PD_KIMAGE}, %esi
        call fill_pxe

        # PDE 0xfc0 ~ ?
        mov ${KERNEL_PD_KIMAGE}, %edi
        mov ${KERNEL_IMAGE_PADDR}, %esi
        lea 0xfc0(%edi), %edi

        mov ${KIMAGE_PAGES}, %ecx
        shr $9, %ecx                # 2M blocks

        or ${PA_PS}, %ebx

    2:
        test %ecx, %ecx
        jz 2f

        call fill_pxe
        lea 8(%edi), %edi
        add $0x200000, %esi
        dec %ecx
        jmp 2b

    2:
        and $(~{PA_PS}), %ebx
        push %esi
        mov ${KERNEL_PT_KIMAGE}, %esi
        call fill_pxe

        # fill PT (kernel image)
        mov ${KERNEL_PT_KIMAGE}, %edi
        pop %esi

        mov ${KIMAGE_PAGES}, %ecx
        and $511, %ecx

    2:
        call fill_pxe
        lea 8(%edi), %edi
        lea 0x1000(%esi), %esi
        loop 2b

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
        mov ${KERNEL_PML4}, %eax
        mov %eax, %cr3

        mov %cr0, %eax
        // SET PE, WP, PG
        or $0x80010001, %eax
        mov %eax, %cr0

        ljmp $0x08, $2f

    # %ebx: attribute low
    # %edx: attribute high
    # %esi: page physical address
    # %edi: page x entry address
    fill_pxe:
        push %eax
        lea (%ebx, %esi, 1), %eax
        mov %eax, (%edi)
        mov %edx, 4(%edi)

        pop %eax
        ret

    .code64
    2:
        jmp {start_64bit}

    .popsection
    "#,
    EARLY_GDT = sym EARLY_GDT,
    EARLY_GDT_DESCRIPTOR = sym EARLY_GDT_DESCRIPTOR,
    BIOS_IDT_DESCRIPTOR = sym BIOS_IDT_DESCRIPTOR,
    KIMAGE_32K_COUNT = sym KIMAGE_32K_COUNT,
    KIMAGE_PAGES = sym KIMAGE_PAGES,
    STAGE1_MAGIC = sym STAGE1_MAGIC,
    STAGE1_MAGIC_VALUE = sym STAGE1_MAGIC_VALUE,
    KERNEL_IMAGE_PADDR = const KERNEL_IMAGE_PADDR,
    KERNEL_PML4 = const KERNEL_PML4,
    PA_P = const PA_P,
    PA_RW = const PA_RW,
    PA_G = const PA_G,
    PA_PS = const PA_PS,
    PA_NXE = const PA_NXE,
    KERNEL_PDPT_PHYS_MAPPING = const KERNEL_PDPT_PHYS_MAPPING,
    KERNEL_PDPT_KERNEL_SPACE = const KERNEL_PDPT_KERNEL_SPACE,
    KERNEL_PD_KIMAGE = const KERNEL_PD_KIMAGE,
    KERNEL_PT_KIMAGE = const KERNEL_PT_KIMAGE,
    start_64bit = sym start_64bit,
    start_32bit = sym start_32bit,
    options(att_syntax),
);

#[unsafe(naked)]
pub unsafe extern "C" fn start_64bit() {
    naked_asm!(
        "mov $0x10, %ax",
        "mov %ax, %ds",
        "mov %ax, %es",
        "mov %ax, %ss",
        "",
        "mov ${kernel_identical_base}, %rax",
        "mov ${stack_paddr}, %rsp",
        "add %rax, %rsp",
        "",
        "xor %rbp, %rbp", // Clear previous stack frame
        "push %rbp", // NULL return address
        "",
        "mov ${e820_data_addr}, %rdi",
        "add %rax, %rdi",
        "",
        "jmp {kernel_init}",
        kernel_identical_base = const 0xffffff0000000000u64,
        stack_paddr = const 0x80000,
        e820_data_addr = sym E820_MEM_MAP_DATA,
        kernel_init = sym init::kernel_init,
        options(att_syntax)
    )
}
