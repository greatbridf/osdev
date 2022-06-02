OUTPUT_FORMAT(elf32-i386)
OUTPUT_ARCH(i386:i386)

MEMORY
{
    WHOLE : org = 0x00000000, l = 4M
}

SECTIONS
{
    .text.bootsect : AT(0x00)
    {
        *(.text.bootsect)
    } > WHOLE

    .magicnumber : AT(0x1fe)
    {
        BYTE(0x55);
        BYTE(0xaa);
    } > WHOLE

    .text.loader 0x7e00 : AT(0x200)
    {
        *(.text.loader)
        __kernel_size_offset = .;
        LONG(LOADADDR(.data) - LOADADDR(.text));
        __loader_end = .;
    } > WHOLE

    .text 0x100000 : AT(LOADADDR(.text.loader) + SIZEOF(.text.loader))
    {
        __real_kernel_start = .;
        *(.text*)
        *(.rodata*)
    } > WHOLE

    .data :
    {
        asm_kernel_size = .;
        LONG(__real_kernel_end - ADDR(.text));

        start_ctors = .;
        KEEP(*(.init_array));
        KEEP(*(SORT_BY_INIT_PRIORITY(.init_array*)));
        end_ctors = .;

        *(.data)
        *(.data*)
    } > WHOLE

    .bss :
    {
        *(.bss)
        *(.bss*)
    } > WHOLE

    .kernel_end :
    {
        __real_kernel_end = .;
    } > WHOLE

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

    .end : AT(0x0ffffff)
    {
        BYTE(0x00);
    } > WHOLE

/*
    /DISCARD/ :
    {
        *(.comment)
    }
*/
}
