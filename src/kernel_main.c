#include <kernel_main.h>

#include <asm/boot.h>
#include <asm/port_io.h>
#include <kernel/event/event.h>
#include <kernel/hw/keyboard.h>
#include <kernel/hw/serial.h>
#include <kernel/hw/timer.h>
#include <kernel/interrupt.h>
#include <kernel/tty.h>
#include <kernel/mem.h>
#include <kernel/stdio.h>
#include <kernel/vga.h>
#include <types/bitmap.h>

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


static struct tty* console = NULL;
#define printkf(x...)                       \
    snprintf(buf, KERNEL_MAIN_BUF_SIZE, x); \
    tty_print(console, buf)

#define EVE_START(x) printkf(x "... ")
#define INIT_START(x) EVE_START("initializing " x)
#define INIT_OK() printkf("ok\n")
#define INIT_FAILED() printkf("failed\n")

static inline void show_mem_info(char* buf)
{
    uint32_t mem_size = 0;
    mem_size += 1024 * asm_mem_size_info.n_1k_blks;
    mem_size += 64 * 1024 * asm_mem_size_info.n_64k_blks;

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
                "[mem] entry %d: %llx ~ %llx, type: %d\n",
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
    printkf("kernel size: %x\n", kernel_size);
}

static inline void check_a20_status(void)
{
    uint32_t result;
    result = check_a20_on();

    if (result) {
        // TODO: change to tty
    } else {
        // TODO: change to tty
    }
}

static inline void halt_on_init_error(void)
{
    MAKE_BREAK_POINT();
    asm_cli();
    while (1)
        asm_hlt();
}

void kernel_main(void)
{
    MAKE_BREAK_POINT();

    char buf[KERNEL_MAIN_BUF_SIZE];

    struct tty early_console;
    if (make_serial_tty(&early_console, PORT_SERIAL0) != GB_OK) {
        halt_on_init_error();
    }
    console = &early_console;

    show_mem_info(buf);

    INIT_START("paging");
    init_paging();
    INIT_OK();

    INIT_START("SSE");
    asm_enable_sse();
    INIT_OK();

    INIT_START("IDT");
    init_idt();
    init_pit();
    INIT_OK();

    INIT_START("heap space");
    if (init_heap() != GB_OK) {
        INIT_FAILED();
        halt_on_init_error();
    }
    INIT_OK();

    INIT_START("C++ global objects");
    call_constructors_for_cpp();
    INIT_OK();

    printkf("Testing k_malloc...\n");
    char* k_malloc_buf = (char*)k_malloc(sizeof(char) * 128);
    snprintf(k_malloc_buf, 128, "This text is printed on the heap!\n");
    // TODO: change to tty
    // vga_printk(k_malloc_buf, 0x0fu);
    k_free(k_malloc_buf);

    INIT_START("serial ports");
    int result = init_serial_port(PORT_SERIAL0);
    if (result == 0) {
        INIT_OK();
    } else {
        INIT_FAILED();
    }

    void* kernel_stack = k_malloc(KERNEL_STACK_SIZE);
    init_gdt_with_tss(kernel_stack + KERNEL_STACK_SIZE - 1, KERNEL_STACK_SEGMENT);
    printkf("new GDT and TSS loaded\n");

    printkf("No work to do, halting...\n");

    while (1) {
        // disable interrupt
        asm_cli();

        dispatch_event();

        asm_sti();
        asm_hlt();
    }
}
