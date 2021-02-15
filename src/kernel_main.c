#include <kernel_main.h>

#include <asm/boot.h>
#include <kernel/stdio.h>
#include <kernel/vga.h>

void kernel_main(void)
{
    asm volatile("xchgw %bx, %bx"); // magic breakpoint
    uint32_t result;
    result = check_a20_on();

    if (result) {
        vga_printk((const int8_t*)"ON", 0x0fU);
    } else {
        vga_printk((const int8_t*)"NOT ON", 0x0fU);
    }

    vga_printk((const int8_t*)"No work to do, halting...", 0x0fU);

_loop:
    asm("hlt");
    goto _loop;
}
