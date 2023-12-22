OUTPUT_FORMAT(elf32-i386)
OUTPUT_ARCH(i386:i386)

MEMORY
{
    MEM : org = 0x00000000, l = 4096M
}

SECTIONS
{
    .stage1 0x8000 : AT(0x00000000)
    {
        __stage1_start = .;
        *(.stage1)

        . = ALIGN(0x1000);
        __stage1_end = .;
    } > MEM

    .kinit :
        AT(LOADADDR(.stage1) + SIZEOF(.stage1))
    {
        __kinit_start = .;
        *(.text.kinit)

        LONG(0x00000000)
        LONG(0x19191919)
        LONG(0x00000000)

        *(.rodata.kinit)

        . = ALIGN(16);

        start_ctors = .;
        KEEP(*(.init_array));
        KEEP(*(SORT_BY_INIT_PRIORITY(.init_array*)));
        KEEP(*(.ctors));
        KEEP(*(SORT_BY_INIT_PRIORITY(.ctors*)));
        end_ctors = .;

        LONG(0x00000000)
        LONG(0x19191919)
        LONG(0x00000000)

        *(.data.kinit)

        LONG(0x00000000)
        LONG(0x19191919)
        LONG(0x00000000)

        *(.bss.kinit)

        LONG(0x00000000)
        LONG(0x19191919)
        LONG(0x00000000)

        . = ALIGN(0x1000);
        __kinit_end = .;
    } > MEM

    .text 0xc0000000 :
        AT(LOADADDR(.kinit) + SIZEOF(.kinit))
    {
        __text_start = .;
        *(.text)
        *(.text*)

        . = ALIGN(0x1000);
        __text_end = .;
    } > MEM

    .rodata :
        AT(LOADADDR(.text) + SIZEOF(.text))
    {
        __rodata_start = .;
        *(.rodata)
        *(.rodata*)

        kmod_loaders_start = .;

        *(.kmods)

        __kmod_loaders_end = .;
        LONG(0);

        . = ALIGN(16);

        bss_addr = .;
        LONG(ABSOLUTE(__bss_start));
        bss_len = .;
        LONG(__bss_end - __bss_start);
        kernel_size = .;
        LONG(__data_end - __kinit_start);

        . = ALIGN(0x1000);
        __rodata_end = .;
    } > MEM

    .data :
        AT(LOADADDR(.rodata) + SIZEOF(.rodata))
    {
        __data_start = .;
        *(.data)
        *(.data*)

        . = ALIGN(0x1000);
        __data_end = .;
    } > MEM

    .bss :
    {
        __bss_start = .;
        *(.bss)
        *(.bss*)

        . = ALIGN(0x1000);
        __bss_end = .;
    } > MEM

    .sentry :
        AT(0x50000)
    { LONG(0x01145140); } > MEM

    .eh_frame :
        AT(LOADADDR(.sentry) + SIZEOF(.sentry))
    {
        __eh_frame_start = .;
        *(.eh_frame*)
        . = ALIGN(0x1000);
        __eh_frame_end = .;
    } > MEM

    /* Stabs debugging sections.  */
    .stab          0 : { *(.stab) }
    .stabstr       0 : { *(.stabstr) }
    .stab.excl     0 : { *(.stab.excl) }
    .stab.exclstr  0 : { *(.stab.exclstr) }
    .stab.index    0 : { *(.stab.index) }
    .stab.indexstr 0 : { *(.stab.indexstr) }
    .comment       0 : { *(.comment) }
    /* DWARF debug sections.
       Symbols in the DWARF debugging sections are relative to the beginning
       of the section so we begin them at 0.  */
    /* DWARF 1 */
    .debug          0 : { *(.debug) }
    .line           0 : { *(.line) }
    /* GNU DWARF 1 extensions */
    .debug_srcinfo  0 : { *(.debug_srcinfo) }
    .debug_sfnames  0 : { *(.debug_sfnames) }
    /* DWARF 1.1 and DWARF 2 */
    .debug_aranges  0 : { *(.debug_aranges) }
    .debug_pubnames 0 : { *(.debug_pubnames) }
    /* DWARF 2 */
    .debug_info     0 : { *(.debug_info .gnu.linkonce.wi.*) }
    .debug_abbrev   0 : { *(.debug_abbrev) }
    .debug_line     0 : { *(.debug_line) }
    .debug_frame    0 : { *(.debug_frame) }
    .debug_str      0 : { *(.debug_str) }
    .debug_loc      0 : { *(.debug_loc) }
    .debug_macinfo  0 : { *(.debug_macinfo) }
    /* SGI/MIPS DWARF 2 extensions */
    .debug_weaknames 0 : { *(.debug_weaknames) }
    .debug_funcnames 0 : { *(.debug_funcnames) }
    .debug_typenames 0 : { *(.debug_typenames) }
    .debug_varnames  0 : { *(.debug_varnames) }

    /DISCARD/ :
    {
        *(.fini_array*)
        *(.note*)
        *(.dtors*)
    }
}
