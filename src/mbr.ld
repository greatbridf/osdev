OUTPUT_FORMAT(binary)
OUTPUT_ARCH(i386:i386)

SECTIONS
{
    .text 0x7c00 :
    {
        *(.text.bootsect)
    }

    /DISCARD/ :
    {
        *(.note*)
    }
}
