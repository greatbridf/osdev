#include <kernel_main.h>

#include <asm/boot.h>

void kernel_main(void)
{
    asm volatile("xchgw %bx, %bx"); // magic breakpoint
    uint32_t result;
    result = check_a20_on();
_loop:
    goto _loop;
}
