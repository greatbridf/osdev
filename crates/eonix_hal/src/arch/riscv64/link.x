SECTIONS {
    .bootstrap ORIGIN(RAM) :
    {
        KEEP(*(.bootstrap.entry));
        KEEP(*(.bootstrap.apentry .bootstrap.data));

        . = ORIGIN(RAM) + 0x1000;
        KEEP(*(.bootstrap.page_table.1));
        KEEP(*(.bootstrap.page_table.2));

        . = ALIGN(16);
        KEEP(*(.bootstrap.stack));
    } > RAM
}
INSERT BEFORE .text;

SECTIONS {
    .text.syscall_fns :
    {

        KEEP(*(.syscall_fns*));

    } > REGION_TEXT AT> RAM
}
INSERT AFTER .text;

SECTIONS {
    .percpu : ALIGN(16)
    {
        __spercpu = .;

        PERCPU_DATA_START = .;

        . = ALIGN(16);

        *(.percpu .percpu*);

        . = ALIGN(16);
        __epercpu = .;
    } > REGION_RODATA AT> RAM

    PERCPU_LENGTH = ABSOLUTE(__epercpu - __spercpu);

    KIMAGE_PAGES = (__edata - _stext + 0x1000 - 1) / 0x1000;
    KIMAGE_32K_COUNT = (KIMAGE_PAGES + 8 - 1) / 8;
    __kernel_end = .;

    BSS_LENGTH = ABSOLUTE(__ebss - __sbss);
}
INSERT AFTER .rodata;

SECTIONS {
    .rodata.syscalls :
    {
        . = ALIGN(16);
        __raw_syscall_handlers_start = .;

        RAW_SYSCALL_HANDLERS = .;
        KEEP(*(.raw_syscalls*));

        __raw_syscall_handlers_end = .;

        RAW_SYSCALL_HANDLERS_SIZE =
            ABSOLUTE(__raw_syscall_handlers_end - __raw_syscall_handlers_start);
    } > REGION_RODATA AT> RAM
}
INSERT AFTER .rodata;

SECTIONS {
    .rodata.fixups :
    {
        . = ALIGN(16);
        FIX_START = .;

        KEEP(*(.fix));

        FIX_END = .;
    } > REGION_RODATA AT> RAM
}
INSERT AFTER .rodata;

SECTIONS {
    .vdso ALIGN(0x1000) : ALIGN(0x1000)
    {
        KEEP(*(.vdso .vdso.*));

        . = ALIGN(0x1000);
    } > VDSO AT> RAM

    VDSO_PADDR = LOADADDR(.vdso);
}
INSERT AFTER .data;
