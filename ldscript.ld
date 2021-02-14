OUTPUT_FORMAT(binary)
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

    .text 0x600 : AT(0x200)
    {
        *(.text.loader)
        *(.text*)
        *(.rodata*)
    } > WHOLE

    .data :
    {
        *(.data)
        *(.data*)
    } > WHOLE

    .bss :
    {
        *(.bss)
        *(.bss*)
    } > WHOLE

    .end : AT(0x0ffffff)
    {
        BYTE(0x00);
    } > WHOLE

    /DISCARD/ :
    {
        *(.comment)
        *(.debug*)
    }
}
