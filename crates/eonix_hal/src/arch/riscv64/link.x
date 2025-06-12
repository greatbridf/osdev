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

        PERCPU_START = .;

        . = ALIGN(16);

        *(.percpu .percpu*);

        . = ALIGN(16);
        __epercpu = .;
    } > LOWMEM AT> REGION_RODATA

    PERCPU_DATA_START = LOADADDR(.percpu);
    PERCPU_LENGTH = ABSOLUTE(__epercpu - __spercpu);

    KIMAGE_PAGES = (__edata - _stext + 0x1000 - 1) / 0x1000;
    KIMAGE_32K_COUNT = (KIMAGE_PAGES + 8 - 1) / 8;
    __kernel_end = .;

    BSS_LENGTH = ABSOLUTE(__ebss - __sbss);
}
INSERT AFTER .rodata;

SECTIONS {
    .bootregion : {
        . = ALIGN(4096);
        *(.bootstack);
        *(.bootdata);
    } > REGION_BOOT
}
INSERT AFTER .rodata;
