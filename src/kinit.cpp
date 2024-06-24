#include <assert.h>
#include <stdint.h>
#include <sys/utsname.h>

#include <types/allocator.hpp>
#include <types/types.h>

#include <kernel/hw/pci.hpp>
#include <kernel/hw/timer.hpp>
#include <kernel/interrupt.hpp>
#include <kernel/log.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/mem/phys.hpp>
#include <kernel/mem/types.hpp>
#include <kernel/process.hpp>
#include <kernel/syscall.hpp>
#include <kernel/utsname.hpp>

using constructor = void (*)();
extern "C" constructor const start_ctors, end_ctors;
extern "C" uint64_t BSS_ADDR, BSS_LENGTH;

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
static inline void set_uname()
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

    set_uname();

    init_interrupt();
    hw::timer::init_pit();

    init_pci();

    // TODO: remove this
    init_vfs();
    // init_syscall();

    init_scheduler();
}

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
    pd[std::get<3>(idx)].set(PA_KERNEL_DATA_HUGE, 0x200000);

    // clear kernel bss
    memset((void*)BSS_ADDR, 0x00, BSS_LENGTH);

    // clear empty page
    memset(mem::physaddr<void>{EMPTY_PAGE_PFN}, 0x00, 0x1000);
}

SECTION(".text.kinit")
static inline void make_early_kernel_stack()
{
    using namespace kernel::mem;
    using namespace kernel::mem::paging;

    constexpr auto idx = idx_all(0xffffffc040000000ULL);

    // early kernel stack
    auto pdpte = KERNEL_PAGE_TABLE[std::get<1>(idx)].parse()[std::get<2>(idx)];
    pdpte.set(PA_KERNEL_PAGE_TABLE, alloc_page_table());

    auto pd = pdpte.parse();
    pd[std::get<3>(idx)].set(PA_KERNEL_DATA_HUGE,
        page_to_pfn(alloc_pages(9)));
}

SECTION(".text.kinit")
static inline void setup_buddy(uintptr_t addr_max)
{
    using namespace kernel::mem;
    using namespace kernel::mem::paging;
    constexpr auto idx = idx_all(0xffffff8040000000ULL);

    addr_max += 0xfff;
    addr_max >>= 12;
    int count = (addr_max * sizeof(page) + 0x200000 - 1) / 0x200000;

    pfn_t start_pfn = 0x400000;

    memset(physaddr<void>{0x105000}, 0x00, 4096);

    auto pdpte = KERNEL_PAGE_TABLE[std::get<1>(idx)].parse()[std::get<2>(idx)];
    pdpte.set(PA_KERNEL_PAGE_TABLE, 0x105000);

    auto pd = pdpte.parse();
    for (int i = 0; i < count; ++i, start_pfn += 0x200000) {
        pd[std::get<3>(idx)+i].set(PA_KERNEL_DATA_HUGE, start_pfn);
    }

    PAGE_ARRAY = (page*)0xffffff8040000000ULL;
    memset(PAGE_ARRAY, 0x00, addr_max * sizeof(page));

    for (int i = 0; i < (int)info::e820_entry_count; ++i) {
        auto& ent = info::e820_entries[i];

        if (ent.type != 1) // type == 1: free area
            continue;
        mark_present(ent.base, ent.base + ent.len);

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

SECTION(".text.kinit")
void setup_gdt()
{
    // user code
    mem::gdt[3]  = 0x0020'fa00'0000'0000;
    // user data
    mem::gdt[4]  = 0x0000'f200'0000'0000;
    // user code32
    mem::gdt[5]  = 0x00cf'fa00'0000'ffff;
    // user data32
    mem::gdt[6]  = 0x00cf'f200'0000'ffff;
    // reserved
    mem::gdt[7]  = 0x0000'0000'0000'0000;

    // TSS descriptor
    mem::gdt[8]  = 0x0000'8900'0070'0067;
    mem::gdt[9]  = 0x0000'0000'0000'0000;

    // LDT descriptor
    mem::gdt[10] = 0x0000'8200'0060'000f;
    mem::gdt[11] = 0x0000'0000'0000'0000;

    // thread local
    mem::gdt[12] = 0x0000'0000'0000'0000;
    mem::gdt[13] = 0x0000'0000'0000'0000;

    uint64_t descriptor[] = {
        0x005f'0000'0000'0000, (uintptr_t)(uint64_t*)mem::gdt
    };

    asm volatile(
            "lgdt (%0)\n\t"
            "mov $0x50, %%ax\n\t"
            "lldt %%ax\n\t"
            "mov $0x40, %%ax\n\t"
            "ltr %%ax\n\t"
            : : "r"((uintptr_t)descriptor+6): "ax"
    );
}

extern "C" SECTION(".text.kinit")
void NORETURN kernel_init(bootloader_data* data)
{
    enable_sse();

    setup_early_kernel_page_table();
    setup_gdt();
    save_memory_info(data);

    uintptr_t addr_max = 0;
    for (int i = 0; i < (int)kernel::mem::info::e820_entry_count; ++i) {
        auto& ent = kernel::mem::info::e820_entries[i];
        if (ent.type != 1)
            continue;
        addr_max = std::max(addr_max, ent.base + ent.len);
    }

    setup_buddy(addr_max);
    init_allocator();

    make_early_kernel_stack();

    asm volatile(
            "mov $0xffffffc040200000, %%rsp\n\t"
            "xor %%rbp, %%rbp\n\t"
            "call *%0\n\t"
            :
            : "r"(real_kernel_init)
            :
            );
    for (;;)
        asm volatile("cli\n\thlt");
}

} // namespace kernel::kinit
