#include <kernel_main.h>

void kernel_main(void)
{
    asm volatile("xchgw %bx, %bx"); // magic breakpoint
_loop:
    goto _loop;
}
