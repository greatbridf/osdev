#include <kernel_main.h>

#include <asm/boot.h>
#include <asm/port_io.h>
#include <kernel/hw/keyboard.h>
#include <kernel/interrupt.h>
#include <kernel/mem.h>
#include <kernel/stdio.h>
#include <kernel/vga.h>

void kernel_main(void)
{
    MAKE_BREAK_POINT();

    uint32_t result;
    result = check_a20_on();

    if (result) {
        vga_printk("A20 is ON\n", 0x0fU);
    } else {
        vga_printk("A20 is NOT ON\n", 0x0fU);
    }

    uint32_t mem_size = 0;
    mem_size += 1024 * asm_mem_size_info.n_1k_blks;
    mem_size += 64 * 1024 * asm_mem_size_info.n_64k_blks;

    char buf[128] = { 0 };
    snprintf(buf, 128, "Memory size: %d bytes (%d MB), 16k blocks: %d, 64k blocks: %d\n",
        mem_size, mem_size / 1024 / 1024, (int32_t)asm_mem_size_info.n_1k_blks,
        (int32_t)asm_mem_size_info.n_64k_blks);
    vga_printk(buf, 0x0fu);

    vga_printk("Initializing interrupt descriptor table...\n", 0x0fu);
    init_idt();

    vga_printk("Interrupt descriptor table initialized!\n", 0x0fu);

    vga_printk("Initializing heap space\n", 0x0fu);

    init_heap();

    vga_printk("Heap space initialized!\n", 0x0fu);

    vga_printk("Testing k_malloc...\n", 0x0fu);
    char* k_malloc_buf = (char*)k_malloc(sizeof(char) * 128);
    snprintf(k_malloc_buf, 128, "This text is printed on the heap!\n");
    vga_printk(k_malloc_buf, 0x0fu);
    k_free(k_malloc_buf);

    vga_printk("No work to do, halting...\n", 0x0fU);

    while (1) {
        // disable interrupt
        asm_cli();

        if (keyboard_has_data()) {
            process_keyboard_data();
        }
        asm_sti();
        asm_hlt();
    }
}
