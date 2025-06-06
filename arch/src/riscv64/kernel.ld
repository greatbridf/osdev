OUTPUT_FORMAT(riscv)
ENTRY(_start)

RAM = 0x80200000;

MEMORY
{
    KBSS          (w)  : org = 0xffffffff00200000, len = 2M
    KIMAGE        (wx) : org = 0xffffffff80200000, len = 2M
    KPERCPU       (w)  : org = 0x0000000000000000, len = 128K
}

SECTIONS
{
    .text : AT(RAM)
    {
        TEXT_START = .;
        *(.text.entry)
        *(.text)
        *(.text*)

        . = ALIGN(0x1000);
        TEXT_END = .;
    } > KIMAGE

    TEXT_PAGES = (TEXT_END - TEXT_START) / 0x1000;

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
        PERCPU_PAGES = .;
        QUAD(_PERCPU_PAGES);

        . = ALIGN(0x1000);
        RODATA_END = .;
    } > KIMAGE

    RODATA_PAGES = (RODATA_END - RODATA_START) / 0x1000;

    .data : AT(LOADADDR(.rodata) + SIZEOF(.rodata))
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
    } > KIMAGE

    DATA_PAGES = (DATA_END - DATA_START) / 0x1000;

    _PERCPU_DATA_START = .;
    .percpu 0 : AT(LOADADDR(.data) + SIZEOF(.data))
    {
        PERCPU_START = .;
        QUAD(0); /* Reserved for x86 percpu pointer */
        QUAD(0);

        *(.percpu .percpu*)

        . = ALIGN(0x1000);
        PERCPU_END = .;
    } > KPERCPU
    _PERCPU_LENGTH = PERCPU_END - PERCPU_START;

    _PERCPU_PAGES = _PERCPU_LENGTH / 0x1000;

    .bss :
    {
        BSS_STACK = .;
        *(.bss.stack)
        BSS_START = .;
        *(.bss)
        *(.bss*)

        . = ALIGN(0x1000);
        BSS_END = .;
    } > KBSS

    KIMAGE_PAGES = TEXT_PAGES + RODATA_PAGES + _PERCPU_PAGES + DATA_PAGES;
    BSS_PAGES = (BSS_END - BSS_START) / 0x1000;
    KERNEL_MAGIC = 0x01145140;

    KIMAGE_32K_COUNT = (KIMAGE_PAGES * 0x1000 + 32 * 1024 - 1) / (32 * 1024);

    .eh_frame :
        AT(LOADADDR(.percpu) + SIZEOF(.percpu))
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
