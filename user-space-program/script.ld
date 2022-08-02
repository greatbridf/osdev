OUTPUT_FORMAT(elf32-i386)
OUTPUT_ARCH(i386:i386)

MEMORY
{
    MEM : org = 0x40000000, l = 3072M

}

SECTIONS
{
    .text :
    {
        *(.text)
        *(.text*)

        __stack_chk_guard = .;
        LONG(0x11451419);
    } > MEM

    /DISCARD/ :
    {
        *(.note.gnu.property*)
    }
}
