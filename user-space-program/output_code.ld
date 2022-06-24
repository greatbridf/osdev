OUTPUT_FORMAT(binary)
OUTPUT_ARCH(i386:i386)

SECTIONS
{
    .text 0x40000000 : AT(0x00)
    {
        *(.text*)
        *(.rodata*)
        *(.data)
        *(.data*)
        *(.bss)
        *(.bss*)
    }

    /DISCARD/ :
    {
        *(.note.gnu.property)
    }
}
