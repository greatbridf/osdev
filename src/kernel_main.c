#include <kernel_main.h>

#include <asm/boot.h>
#include <kernel/mem.h>
#include <kernel/stdio.h>
#include <kernel/vga.h>

void kernel_main(void)
{
    MAKE_BREAK_POINT();

    uint32_t result;
    result = check_a20_on();

    if (result) {
        vga_printk((const int8_t*)"A20 is ON\n", 0x0fU);
    } else {
        vga_printk((const int8_t*)"A20 is NOT ON\n", 0x0fU);
    }

    uint32_t mem_size = 0;
    mem_size += 1024 * asm_mem_size_info.n_1k_blks;
    mem_size += 64 * 1024 * asm_mem_size_info.n_64k_blks;

    char buf[128] = { 0 };
    snprintf(buf, 128, "Memory size: %d bytes (%d MB), 16k blocks: %d, 64k blocks: %d\n",
        mem_size, mem_size / 1024 / 1024, (int32_t)asm_mem_size_info.n_1k_blks,
        (int32_t)asm_mem_size_info.n_64k_blks);
    vga_printk((const int8_t*)buf, 0x0fu);

    vga_printk((const int8_t*)"No work to do, halting...\n", 0x0fU);

_loop:
    asm("hlt");
    goto _loop;
}
