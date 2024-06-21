#include <asm/port_io.h>

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/utsname.h>

#include <types/types.h>

#include <kernel/hw/keyboard.h>
#include <kernel/hw/pci.hpp>
#include <kernel/hw/serial.h>
#include <kernel/hw/timer.h>
#include <kernel/interrupt.h>
#include <kernel/log.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/mem/types.hpp>
#include <kernel/process.hpp>
#include <kernel/syscall.hpp>
#include <kernel/task.h>
#include <kernel/tty.hpp>
#include <kernel/utsname.hpp>
#include <kernel/vga.hpp>

typedef void (*constructor)(void);
extern constructor const SECTION(".rodata.kinit") start_ctors;
extern constructor const SECTION(".rodata.kinit") end_ctors;

struct PACKED bootloader_data {
    uint32_t meminfo_entry_count;
    uint32_t meminfo_entry_length;

    // don't forget to add the initial 1m to the total
    uint32_t meminfo_1k_blocks;
    uint32_t meminfo_64k_blocks;

    // meminfo entries
    kernel::mem::e820_mem_map_entry
        meminfo_entries[(1024-4*4)/24];
};

extern void init_vfs();

namespace kernel::kinit {

SECTION(".text.kinit")
static inline void enable_sse()
{
    asm volatile(
            "mov %%cr0, %%rax\n\t"
            "and $(~0xc), %%rax\n\t"
            "or $0x22, %%rax\n\t"
            "mov %%rax, %%cr0\n\t"
            "\n\t"
            "mov %%cr4, %%rax\n\t"
            "or $0x600, %%rax\n\t"
            "mov %%rax, %%cr4\n\t"
            "fninit\n\t"
            ::: "rax"
            );
}

SECTION(".text.kinit")
static inline int init_console(const char* name)
{
    if (name[0] == 't' && name[1] == 't' && name[2] == 'y') {
        if (name[3] == 'S' || name[3] == 's') {
            if (name[4] == '0') {
                console = new serial_tty(PORT_SERIAL0);
                return 0;
            }
            if (name[4] == '1') {
                console = new serial_tty(PORT_SERIAL1);
                return 0;
            }
        }
        if (name[3] == 'V' && name[3] == 'G' && name[3] == 'A') {
            console = new vga_tty{};
            return 0;
        }
    }
    return -EINVAL;
}

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

SECTION(".text.kinit")
void NORETURN real_kernel_init()
{
    // call global constructors
    // NOTE: the initializer of global objects MUST NOT contain
    // all kinds of memory allocations
    for (auto* ctor = &start_ctors; ctor != &end_ctors; ++ctor)
        (*ctor)();

    init_idt();
    // TODO: LONG MODE
    // init_mem();
    init_pic();
    init_pit();

    init_uname();

    int ret = init_serial_port(PORT_SERIAL0);
    assert(ret == 0);

    ret = init_console("ttyS0");
    assert(ret == 0);

    kernel::kinit::init_pci();
    init_vfs();
    // TODO: LONG MODE
    // init_syscall();

    kmsg("switching execution to the scheduler...\n");
    init_scheduler();
}

extern "C" uint64_t BSS_ADDR;
extern "C" uint64_t BSS_LENGTH;

SECTION(".text.kinit")
static inline void setup_early_kernel_page_table()
{
    using namespace kernel::mem::paging;

    // remove temporary mapping
    KERNEL_PAGE_TABLE[0x000].clear();

    constexpr auto idx = idx_all(0xffffffffc0200000ULL);

    auto pdpt = KERNEL_PAGE_TABLE[std::get<1>(idx)].parse();
    auto pd = pdpt[std::get<2>(idx)].parse();

    // kernel bss, size 2M
    pd[std::get<3>(idx)].set(PA_P | PA_RW | PA_PS | PA_G | PA_NXE, 0x200000);

    // clear kernel bss
    memset((void*)BSS_ADDR, 0x00, BSS_LENGTH);
}

SECTION(".text.kinit")
static inline void make_early_kernel_stack()
{
    using namespace kernel::mem;
    using namespace kernel::mem::paging;

    auto* kstack_pdpt_page = alloc_page();
    auto* kstack_page = alloc_pages(9);

    memset(physaddr<char>{page_to_pfn(kstack_pdpt_page)}, 0x00, 0x1000);

    constexpr auto idx = idx_all(0xffffffc040000000ULL);

    // early kernel stack
    auto pdpte = KERNEL_PAGE_TABLE[std::get<1>(idx)].parse()[std::get<2>(idx)];
    pdpte.set(PA_P | PA_RW | PA_G | PA_NXE, page_to_pfn(kstack_pdpt_page));

    auto pd = pdpte.parse();
    pd[std::get<3>(idx)].set(
            PA_P | PA_RW | PA_PS | PA_G | PA_NXE,
            page_to_pfn(kstack_page));
}

SECTION(".text.kinit")
static inline void setup_buddy(uintptr_t addr_max)
{
    using namespace kernel::mem;
    using namespace kernel::mem::paging;
    constexpr auto idx = idx_all(0xffffff8040000000ULL);

    addr_max >>= 12;
    int count = (addr_max * sizeof(page) + 0x200000 - 1) / 0x200000;

    pfn_t start_pfn = 0x400000;

    memset(physaddr<void>{0x105000}, 0x00, 4096);

    auto pdpte = KERNEL_PAGE_TABLE[std::get<1>(idx)].parse()[std::get<2>(idx)];
    pdpte.set(PA_P | PA_RW | PA_G | PA_NXE, 0x105000);

    auto pd = pdpte.parse();
    for (int i = 0; i < count; ++i, start_pfn += 0x200000) {
        pd[std::get<3>(idx)+i].set(
            PA_P | PA_RW | PA_PS | PA_G | PA_NXE, start_pfn);
    }

    PAGE_ARRAY = (page*)0xffffff8040000000ULL;
    memset(PAGE_ARRAY, 0x00, addr_max * sizeof(page));

    for (int i = 0; i < (int)info::e820_entry_count; ++i) {
        auto& ent = info::e820_entries[i];
        if (ent.type != 1) // type == 1: free area
            continue;

        auto start = ent.base;
        auto end = start + ent.len;
        if (end <= 0x106000)
            continue;

        if (start < 0x106000)
            start = 0x106000;

        if (start < 0x200000 && end >= 0x200000) {
            mem::paging::create_zone(start, 0x200000);
            start = start_pfn;
        }

        if (start > end)
            continue;

        mem::paging::create_zone(start, end);
    }
}

SECTION(".text.kinit")
static inline void save_memory_info(bootloader_data* data)
{
    kernel::mem::info::memory_size = 1ULL * 1024ULL * 1024ULL + // initial 1M
        1024ULL * data->meminfo_1k_blocks + 64ULL * 1024ULL * data->meminfo_64k_blocks;
    kernel::mem::info::e820_entry_count = data->meminfo_entry_count;
    kernel::mem::info::e820_entry_length = data->meminfo_entry_length;

    memcpy(kernel::mem::info::e820_entries, data->meminfo_entries,
        sizeof(kernel::mem::info::e820_entries));
}

extern "C" SECTION(".text.kinit")
void NORETURN kernel_init(bootloader_data* data)
{
    enable_sse();

    setup_early_kernel_page_table();
    save_memory_info(data);

    // create struct pages
    uintptr_t addr_max = 0;
    for (int i = 0; i < (int)kernel::mem::info::e820_entry_count; ++i) {
        auto& ent = kernel::mem::info::e820_entries[i];
        if (ent.type != 1)
            continue;
        addr_max = std::max(addr_max, ent.base + ent.len);
    }

    setup_buddy(addr_max);
    make_early_kernel_stack();

    asm volatile(
            "mov $0xffffffc040200000, %%rsp\n\t"
            "xor %%rbp, %%rbp\n\t"
            "call *%0\n\t"
            :
            : "r"(real_kernel_init)
            :
            );
    die();
}

} // namespace kernel::kinit
