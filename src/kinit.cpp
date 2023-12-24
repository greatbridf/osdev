#include <asm/port_io.h>
#include <asm/sys.h>

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/utsname.h>

#include <types/status.h>
#include <types/types.h>

#include <kernel/event/event.h>
#include <kernel/hw/keyboard.h>
#include <kernel/hw/pci.hpp>
#include <kernel/hw/serial.h>
#include <kernel/hw/timer.h>
#include <kernel/interrupt.h>
#include <kernel/log.hpp>
#include <kernel/mem.h>
#include <kernel/process.hpp>
#include <kernel/syscall.hpp>
#include <kernel/task.h>
#include <kernel/tty.hpp>
#include <kernel/utsname.hpp>
#include <kernel/vga.hpp>

typedef void (*constructor)(void);
extern constructor const SECTION(".rodata.kinit") start_ctors;
extern constructor const SECTION(".rodata.kinit") end_ctors;

extern struct mem_size_info SECTION(".stage1") asm_mem_size_info;
extern uint8_t SECTION(".stage1") asm_e820_mem_map[1024];
extern uint32_t SECTION(".stage1") asm_e820_mem_map_count;
extern uint32_t SECTION(".stage1") asm_e820_mem_map_entry_size;

SECTION(".text.kinit")
static inline void save_loader_data(void)
{
    memcpy(e820_mem_map, asm_e820_mem_map, sizeof(e820_mem_map));
    e820_mem_map_count = asm_e820_mem_map_count;
    e820_mem_map_entry_size = asm_e820_mem_map_entry_size;
    memcpy(&mem_size_info, &asm_mem_size_info, sizeof(struct mem_size_info));
}

SECTION(".text.kinit")
static inline void load_new_gdt(void)
{
    create_segment_descriptor(gdt + 0, 0, 0, 0, 0);
    create_segment_descriptor(gdt + 1, 0, ~0, 0b1100, SD_TYPE_CODE_SYSTEM);
    create_segment_descriptor(gdt + 2, 0, ~0, 0b1100, SD_TYPE_DATA_SYSTEM);
    create_segment_descriptor(gdt + 3, 0, ~0, 0b1100, SD_TYPE_CODE_USER);
    create_segment_descriptor(gdt + 4, 0, ~0, 0b1100, SD_TYPE_DATA_USER);
    create_segment_descriptor(gdt + 5, (uint32_t)&tss, sizeof(tss), 0b0000, SD_TYPE_TSS);
    create_segment_descriptor(gdt + 6, 0, 0, 0b1100, SD_TYPE_DATA_USER);

    asm_load_gdt((7 * 8 - 1) << 16, (pptr_t)gdt);
    asm_load_tr((6 - 1) * 8);

    asm_cli();
}

SECTION(".text.kinit")
static inline void init_bss_section(void)
{
    memset(bss_addr, 0x00, bss_len);
}

SECTION(".text.kinit")
static inline int init_console(const char* name)
{
    if (name[0] == 't' && name[1] == 't' && name[2] == 'y') {
        if (name[3] == 'S' || name[3] == 's') {
            if (name[4] == '0') {
                console = types::memory::kinew<serial_tty>(PORT_SERIAL0);
                return GB_OK;
            }
            if (name[4] == '1') {
                console = types::memory::kinew<serial_tty>(PORT_SERIAL1);
                return GB_OK;
            }
        }
        if (name[3] == 'V' && name[3] == 'G' && name[3] == 'A') {
            console = types::memory::kinew<vga_tty>();
            return GB_OK;
        }
    }
    return GB_FAILED;
}

extern void init_vfs();

namespace kernel::kinit {

SECTION(".text.kinit")
static void init_uname()
{
    kernel::sys_utsname = new new_utsname;
    strcpy(kernel::sys_utsname->sysname, "Linux"); // linux compatible
    strcpy(kernel::sys_utsname->nodename, "(none)");
    strcpy(kernel::sys_utsname->release, "1.0.0");
    strcpy(kernel::sys_utsname->version, "1.0.0");
    strcpy(kernel::sys_utsname->machine, "x86");
    strcpy(kernel::sys_utsname->domainname, "(none)");
}

} // namespace kernel::kinit

extern "C" SECTION(".text.kinit") void NORETURN kernel_init(void)
{
    asm_enable_sse();

    init_bss_section();

    save_loader_data();

    load_new_gdt();

    // call global ctors
    // NOTE:
    // the initializer of global objects MUST NOT contain
    // all kinds of memory allocations
    for (const constructor* ctor = &start_ctors; ctor != &end_ctors; ++ctor) {
        (*ctor)();
    }

    init_idt();
    init_mem();
    init_pic();
    init_pit();

    kernel::kinit::init_uname();

    int ret = init_serial_port(PORT_SERIAL0);
    assert(ret == GB_OK);

    ret = init_console("ttyS0");
    assert(ret == GB_OK);

    kernel::kinit::init_pci();
    init_vfs();
    init_syscall();

    kmsg("switching execution to the scheduler...\n");
    init_scheduler();
}
