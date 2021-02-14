OUTPUT_FORMAT(binary)
OUTPUT_ARCH(i386:i386)

SECTIONS
{
    .text :
    {
        *(.text.bootsect)
        *(.text.loader)
        *(.text*)
        *(.rodata)
    }

    .data :
    {
        *(.data)
    }

    .bss :
    {
        *(.bss)
    }

    .end : AT(0x0ffffff)
    {
        BYTE(0x00);
    }

    /DISCARD/ :
    {
        *(.comment)
    }
}
