#include "kernel_main.hpp"

#include <asm/boot.h>
#include <asm/port_io.h>
#include <asm/sys.h>
#include <kernel/event/event.h>
#include <kernel/hw/keyboard.h>
#include <kernel/hw/serial.h>
#include <kernel/hw/timer.h>
#include <kernel/interrupt.h>
#include <kernel/mem.h>
#include <kernel/process.hpp>
#include <kernel/stdio.hpp>
#include <kernel/task.h>
#include <kernel/tty.hpp>
#include <kernel/vga.hpp>
#include <types/assert.h>
#include <types/bitmap.h>
#include <types/status.h>
#include <types/stdint.h>
#include <types/types.h>

#define KERNEL_MAIN_BUF_SIZE (128)

#define printkf(x...)                       \
    snprintf(buf, KERNEL_MAIN_BUF_SIZE, x); \
    console->print(buf)

typedef void (*constructor)(void);
extern constructor start_ctors;
extern constructor end_ctors;
void call_constructors_for_cpp(void)
{
    for (constructor* ctor = &start_ctors; ctor != &end_ctors; ++ctor) {
        (*ctor)();
    }
}

uint8_t e820_mem_map[1024];
uint32_t e820_mem_map_count;
uint32_t e820_mem_map_entry_size;
size_t kernel_size;
struct mem_size_info mem_size_info;

static inline void save_loader_data(void)
{
    memcpy(e820_mem_map, asm_e820_mem_map, sizeof(e820_mem_map));
    e820_mem_map_count = asm_e820_mem_map_count;
    e820_mem_map_entry_size = asm_e820_mem_map_entry_size;
    kernel_size = asm_kernel_size;
    memcpy(&mem_size_info, &asm_mem_size_info, sizeof(struct mem_size_info));
}

static inline void show_mem_info(char* buf)
{
    uint32_t mem_size = 0;
    mem_size += 1024 * mem_size_info.n_1k_blks;
    mem_size += 64 * 1024 * mem_size_info.n_64k_blks;

    printkf(
        "Memory size: %d bytes (%d MB), 16k blocks: %d, 64k blocks: %d\n",
        mem_size,
        mem_size / 1024 / 1024,
        (int32_t)mem_size_info.n_1k_blks,
        (int32_t)mem_size_info.n_64k_blks);

    printkf(
        "mem_map_entry_count: %d , mem_map_entry_size: %d \n",
        e820_mem_map_count,
        e820_mem_map_entry_size);

    if (e820_mem_map_entry_size == 20) {
        struct e820_mem_map_entry_20* entry = (struct e820_mem_map_entry_20*)e820_mem_map;
        for (uint32_t i = 0; i < e820_mem_map_count; ++i, ++entry) {
            printkf(
                "[mem] entry %d: %llx ~ %llx, type: %d\n",
                i,
                entry->base,
                entry->base + entry->len,
                entry->type);
        }
    } else {
        struct e820_mem_map_entry_24* entry = (struct e820_mem_map_entry_24*)e820_mem_map;
        for (uint32_t i = 0; i < e820_mem_map_count; ++i, ++entry) {
            printkf(
                "[mem] entry %d: %lld ~ %lld, type: %d, acpi_attr: %d\n",
                i,
                entry->in.base,
                entry->in.base + entry->in.len,
                entry->in.type,
                entry->acpi_extension_attr);
        }
    }
    printkf("kernel size: %x\n", kernel_size);
}

static segment_descriptor new_gdt[6];
struct tss32_t tss;

void load_new_gdt(void)
{
    create_segment_descriptor(new_gdt + 0, 0, 0, 0, 0);
    create_segment_descriptor(new_gdt + 1, 0, ~0, 0b1100, SD_TYPE_CODE_SYSTEM);
    create_segment_descriptor(new_gdt + 2, 0, ~0, 0b1100, SD_TYPE_DATA_SYSTEM);
    create_segment_descriptor(new_gdt + 3, 0, ~0, 0b1100, SD_TYPE_CODE_USER);
    create_segment_descriptor(new_gdt + 4, 0, ~0, 0b1100, SD_TYPE_DATA_USER);
    create_segment_descriptor(new_gdt + 5, (uint32_t)&tss, sizeof(tss), 0b0000, SD_TYPE_TSS);

    asm_load_gdt((6 * 8 - 1) << 16, (pptr_t)new_gdt);
    asm_load_tr((6 - 1) * 8);

    asm_cli();
}

void init_bss_section(void)
{
    void* bss_addr = (void*)bss_section_start_addr;
    size_t bss_size = bss_section_end_addr - bss_section_start_addr;
    memset(bss_addr, 0x00, bss_size);
}

int init_console(const char* name)
{
    if (name[0] == 't' && name[1] == 't' && name[2] == 'y') {
        if (name[3] == 'S' || name[3] == 's') {
            if (name[4] == '0') {
                console = types::_new<types::kernel_ident_allocator, serial_tty>(PORT_SERIAL0);
                return GB_OK;
            }
            if (name[4] == '1') {
                console = types::_new<types::kernel_ident_allocator, serial_tty>(PORT_SERIAL1);
                return GB_OK;
            }
        }
        if (name[3] == 'V' && name[3] == 'G' && name[3] == 'A') {
            console = types::_new<types::kernel_ident_allocator, vga_tty>();
            return GB_OK;
        }
    }
    crash();
    return GB_FAILED;
}

extern void init_vfs();
extern "C" uint32_t check_a20_on(void);

extern "C" void NORETURN kernel_main(void)
{
    assert(check_a20_on());

    asm_enable_sse();

    init_bss_section();

    save_loader_data();

    load_new_gdt();

    // NOTE:
    // the initializer of c++ global objects MUST NOT contain
    // all kinds of memory allocations
    call_constructors_for_cpp();

    char buf[KERNEL_MAIN_BUF_SIZE] = { 0 };

    assert(init_serial_port(PORT_SERIAL0) == GB_OK);

    init_idt();
    init_mem();
    init_pic();
    init_pit();

    assert(init_console("ttyS0") == GB_OK);

    show_mem_info(buf);

    init_vfs();

    kmsg("switching execution to the scheduler...\n");
    init_scheduler();
}

void NORETURN __stack_chk_fail(void)
{
    kmsg("***** stack smashing detected! *****\n");
    for (;;)
        assert(0);
}

extern "C" void NORETURN __cxa_pure_virtual(void)
{
    for (;;)
        assert(0);
}

void crash(void)
{
    asm volatile("ud2");
}

void _debugger_breakpoint(void)
{
#ifdef __BOCHS_SYSTEM__
    asm volatile("xchgw %%bx, %%bx");
#else
    asm volatile("nop");
#endif
}
