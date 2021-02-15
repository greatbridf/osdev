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
        vga_printk((const int8_t*)"A20 is ON\n", 0x0fU);
    } else {
        vga_printk((const int8_t*)"A20 is NOT ON\n", 0x0fU);
    }

    vga_printk((const int8_t*)"No work to do, halting...\n", 0x0fU);

_loop:
    asm("hlt");
    goto _loop;
}
