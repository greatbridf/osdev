OUTPUT_FORMAT(elf64-x86-64)

MEMORY
{
    MBR    (wx) : org = 0x0e00, l = 512
    STAGE1 (wx) : org = 0x1000, l = 4K
    PHYMEM (w)  : org = 0xffffff0000000000, len = 512 * 1024M
    PARRAY (w)  : org = 0xffffff8000000000, len = 128 * 1024M
    KBSS   (w)  : org = 0xffffffffc0200000, len = 2M
    KIMAGE (wx) : org = 0xffffffffffc00000, len = 2M
}

SECTIONS
{
    .mbr : AT(0)
    {
        KEEP(*(.mbr));

        . = 446;
        BYTE(0x00);

        . = 510;
        BYTE(0x55);
        BYTE(0xaa);
    } > MBR

    .stage1 : AT(LOADADDR(.mbr) + SIZEOF(.mbr))
    {
        *(.stage1)
        . = ALIGN(0x1000);
    } > STAGE1

    .text :
        AT(LOADADDR(.stage1) + SIZEOF(.stage1))
    {
        KIMAGE_START = .;
        TEXT_START = .;
        *(.text)
        *(.text*)

        . = ALIGN(0x1000);
        TEXT_END = .;
    } > KIMAGE

    .rodata :
        AT(LOADADDR(.text) + SIZEOF(.text))
    {
        RODATA_START = .;
        *(.rodata)
        *(.rodata*)

        . = ALIGN(16);
        start_ctors = .;
        KEEP(*(.init_array));
        KEEP(*(SORT_BY_INIT_PRIORITY(.init_array*)));
        KEEP(*(.ctors));
        KEEP(*(SORT_BY_INIT_PRIORITY(.ctors*)));
        end_ctors = .;

        . = ALIGN(16);
        _fix_start = .;
        KEEP(*(.fix));
        _fix_end = .;

        . = ALIGN(16);
        BSS_ADDR = .;
        QUAD(ABSOLUTE(BSS_START));
        BSS_LENGTH = .;
        QUAD(BSS_END - BSS_START);
        FIX_START = .;
        QUAD(ABSOLUTE(_fix_start));
        FIX_END = .;
        QUAD(ABSOLUTE(_fix_end));

        . = ALIGN(0x1000);
        RODATA_END = .;
    } > KIMAGE

    .data :
        AT(LOADADDR(.rodata) + SIZEOF(.rodata))
    {
        DATA_START = .;
        *(.data)
        *(.data*)

        *(.got)
        *(.got.plt)

        . = . + 4;
        . = ALIGN(0x1000) - 4;
        LONG(KERNEL_MAGIC);

        DATA_END = .;
        KIMAGE_END = .;
    } > KIMAGE

    .bss :
    {
        BSS_START = .;
        *(.bss)
        *(.bss*)

        . = ALIGN(0x1000);
        BSS_END = .;
    } > KBSS

    KIMAGE_PAGES = (KIMAGE_END - KIMAGE_START) / 0x1000;
    BSS_PAGES = (BSS_END - BSS_START) / 0x1000;
    KERNEL_MAGIC = 0x01145140;

    KIMAGE_32K_COUNT = ((KIMAGE_END - KIMAGE_START) + 32 * 1024 - 1) / (32 * 1024);

    .eh_frame :
        AT(LOADADDR(.data) + SIZEOF(.data))
    {
        KEEP(*(.eh_frame*))
        . = ALIGN(0x1000);
    } > KIMAGE

    /* Stabs debugging sections.  */
    .stab          0 : { KEEP(*(.stab)); }
    .stabstr       0 : { KEEP(*(.stabstr)); }
    .stab.excl     0 : { KEEP(*(.stab.excl)); }
    .stab.exclstr  0 : { KEEP(*(.stab.exclstr)); }
    .stab.index    0 : { KEEP(*(.stab.index)); }
    .stab.indexstr 0 : { KEEP(*(.stab.indexstr)); }
    .comment       0 : { KEEP(*(.comment)); }
    /* DWARF debug sections.
       Symbols in the DWARF debugging sections are relative to the beginning
       of the section so we begin them at 0.  */
    /* DWARF 1 */
    .debug          0 : { KEEP(*(.debug)); }
    .line           0 : { KEEP(*(.line)); }
    /* GNU DWARF 1 extensions */
    .debug_srcinfo  0 : { KEEP(*(.debug_srcinfo)); }
    .debug_sfnames  0 : { KEEP(*(.debug_sfnames)); }
    /* DWARF 1.1 and DWARF 2 */
    .debug_aranges  0 : { KEEP(*(.debug_aranges)); }
    .debug_pubnames 0 : { KEEP(*(.debug_pubnames)); }
    /* DWARF 2 */
    .debug_info     0 : { KEEP(*(.debug_info)); }
    .debug_abbrev   0 : { KEEP(*(.debug_abbrev)); }
    .debug_line     0 : { KEEP(*(.debug_line)); }
    .debug_frame    0 : { KEEP(*(.debug_frame)); }
    .debug_str      0 : { KEEP(*(.debug_str)); }
    .debug_loc      0 : { KEEP(*(.debug_loc)); }
    .debug_macinfo  0 : { KEEP(*(.debug_macinfo)); }
    /* SGI/MIPS DWARF 2 extensions */
    .debug_weaknames 0 : { KEEP(*(.debug_weaknames)); }
    .debug_funcnames 0 : { KEEP(*(.debug_funcnames)); }
    .debug_typenames 0 : { KEEP(*(.debug_typenames)); }
    .debug_varnames  0 : { KEEP(*(.debug_varnames)); }

    /* DWARF Other */
    .debug_ranges  0 : { KEEP(*(.debug_ranges)); }
    .debug_line_str 0 : { KEEP(*(.debug_line_str)); }
    /* Rust stuff */

    /DISCARD/ :
    {
        *(.fini_array*)
        *(.note*)
        *(.dtors*)
        *(.debug_gdb_scripts*)
    }
}
