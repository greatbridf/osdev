#include <kernel_main.h>

#include <asm/boot.h>
#include <asm/port_io.h>
#include <kernel/event/event.h>
#include <kernel/hw/keyboard.h>
#include <kernel/hw/timer.h>
#include <kernel/interrupt.h>
#include <kernel/mem.h>
#include <kernel/stdio.h>
#include <kernel/vga.h>

typedef void (*constructor)(void);
extern constructor start_ctors;
extern constructor end_ctors;
void call_constructors_for_cpp(void)
{
    for (constructor* ctor = &start_ctors; ctor != &end_ctors; ++ctor) {
        (*ctor)();
    }
}

#define KERNEL_MAIN_BUF_SIZE (128)

#define printkf(x...)                       \
    snprintf(buf, KERNEL_MAIN_BUF_SIZE, x); \
    vga_printk(buf, 0x0fu);

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

    char buf[KERNEL_MAIN_BUF_SIZE] = { 0 };
    printkf(
        "Memory size: %d bytes (%d MB), 16k blocks: %d, 64k blocks: %d\n",
        mem_size,
        mem_size / 1024 / 1024,
        (int32_t)asm_mem_size_info.n_1k_blks,
        (int32_t)asm_mem_size_info.n_64k_blks);

    printkf(
        "mem_map_entry_count: %d , mem_map_entry_size: %d \n",
        asm_e820_mem_map_count,
        asm_e820_mem_map_entry_size);

    if (asm_e820_mem_map_entry_size == 20) {
        for (uint32_t i = 0; i < asm_e820_mem_map_count; ++i) {
            printkf(
                "[mem] entry %d: %lld ~ %lld, type: %d\n",
                i,
                e820_mem_map_20[i].base,
                e820_mem_map_20[i].base + e820_mem_map_20[i].len,
                e820_mem_map_20[i].type);
        }
    } else {
        for (uint32_t i = 0; i < asm_e820_mem_map_count; ++i) {
            printkf(
                "[mem] entry %d: %lld ~ %lld, type: %d, acpi_attr: %d\n",
                i,
                e820_mem_map_24[i].in.base,
                e820_mem_map_24[i].in.base + e820_mem_map_24[i].in.len,
                e820_mem_map_24[i].in.type,
                e820_mem_map_24[i].acpi_extension_attr);
        }
    }

    init_paging();

    printkf("Paging enabled\n");

    printkf("Initializing interrupt descriptor table...\n");

    init_idt();

    init_pit();

    printkf("Interrupt descriptor table initialized!\n");

    printkf("Initializing heap space\n");

    init_heap();

    printkf("Heap space initialized!\n");

    printkf("Constructing c++ global objects\n");

    call_constructors_for_cpp();

    printkf("Cpp global objects constructed\n");

    printkf("Testing k_malloc...\n");
    char* k_malloc_buf = (char*)k_malloc(sizeof(char) * 128);
    snprintf(k_malloc_buf, 128, "This text is printed on the heap!\n");
    vga_printk(k_malloc_buf, 0x0fu);
    k_free(k_malloc_buf);

    printkf("No work to do, halting...\n");

    while (1) {
        // disable interrupt
        asm_cli();

        dispatch_event();

        asm_sti();
        asm_hlt();
    }
}
