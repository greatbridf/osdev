SECTIONS {
    .low 0x500 (NOLOAD) :
    {

        KEEP(*(.low .low*));

    } > LOWMEM

    .mbr 0xe00 :
    {
        KEEP(*(.mbr));

        /* avoid the MBR being overwritten */
        . = ABSOLUTE(ADDR(.mbr) + 446);

        /* avoid the MBR being overwritten */
        . = ABSOLUTE(ADDR(.mbr) + 510);
        BYTE(0x55);
        BYTE(0xaa);
    } > LOWMEM = 0x00

    .stage1 0x6000 :
    {
        KEEP(*(.stage1.smp));

        . = ALIGN(16);
        KEEP(*(.stage1));

        . = ABSOLUTE(ADDR(.stage1) + 512 * 7 - 4);
        STAGE1_MAGIC = .;
        LONG(ABSOLUTE(STAGE1_MAGIC_VALUE));

        STAGE1_MAGIC_VALUE = 0x01145140;
    } > LOWMEM AT> LOWMEM
}

SECTIONS {
    .text.syscall_fns :
    {

        KEEP(*(.syscall_fns*));

    } > REGION_TEXT
}
INSERT AFTER .text;

SECTIONS {
    .rodata.fixups :
    {
        . = ALIGN(16);
        FIX_START = .;

        KEEP(*(.fix));

        FIX_END = .;
    } > REGION_RODATA

    .rodata.syscalls :
    {
        . = ALIGN(16);
        __raw_syscall_handlers_start = .;

        RAW_SYSCALL_HANDLERS = .;
        KEEP(*(.raw_syscalls*));

        __raw_syscall_handlers_end = .;

        RAW_SYSCALL_HANDLERS_SIZE =
            ABSOLUTE(__raw_syscall_handlers_end - __raw_syscall_handlers_start);
    } > REGION_RODATA
}
INSERT AFTER .rodata;

SECTIONS {
    .percpu 0 : ALIGN(16)
    {
        __spercpu = .;

        QUAD(0); /* Reserved for x86 percpu pointer */

        . = ALIGN(16);

        *(.percpu .percpu*);

        . = ALIGN(16);
        __epercpu = .;
    } > LOWMEM AT> REGION_RODATA

    PERCPU_DATA_START = LOADADDR(.percpu);
    PERCPU_LENGTH = ABSOLUTE(__epercpu - __spercpu);

    KIMAGE_PAGES = (__edata - _stext + 0x1000 - 1) / 0x1000;
    KIMAGE_32K_COUNT = (KIMAGE_PAGES + 8 - 1) / 8;

    BSS_LENGTH = ABSOLUTE(__ebss - __sbss);
}
INSERT AFTER .rodata;
