#include <stdint.h>
#include <sys/utsname.h>

#include <types/allocator.hpp>
#include <types/types.h>

#include <kernel/hw/acpi.hpp>
#include <kernel/hw/pci.hpp>
#include <kernel/interrupt.hpp>
#include <kernel/log.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/mem/phys.hpp>
#include <kernel/mem/types.hpp>
#include <kernel/process.hpp>
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
    kernel::mem::e820_mem_map_entry meminfo_entries[(1024 - 4 * 4) / 24];
};

namespace kernel::kinit {

static inline void enable_sse() {
    asm volatile(
        "mov %%cr0, %%rax\n\t"
        "and $(~0xc), %%rax\n\t"
        "or $0x22, %%rax\n\t"
        "mov %%rax, %%cr0\n\t"
        "\n\t"
        "mov %%cr4, %%rax\n\t"
        "or $0x600, %%rax\n\t"
        "mov %%rax, %%cr4\n\t"
        "fninit\n\t" ::
            : "rax");
}

static inline void setup_early_kernel_page_table() {
    using namespace kernel::mem::paging;

    constexpr auto idx = idx_all(0xffffffffc0200000ULL);

    auto pdpt = KERNEL_PAGE_TABLE[std::get<1>(idx)].parse();
    auto pd = pdpt[std::get<2>(idx)].parse();

    // kernel bss, size 2M
    pd[std::get<3>(idx)].set(PA_KERNEL_DATA_HUGE, KERNEL_BSS_HUGE_PAGE);

    // clear kernel bss
    memset((void*)BSS_ADDR, 0x00, BSS_LENGTH);
}

extern "C" char KIMAGE_PAGES[];

static inline void setup_buddy(uintptr_t addr_max) {
    using namespace kernel::mem;
    using namespace kernel::mem::paging;
    constexpr auto idx = idx_all(0xffffff8040000000ULL);

    addr_max += 0xfff;
    addr_max >>= 12;
    int count = (addr_max * sizeof(page) + 0x200000 - 1) / 0x200000;

    auto KIMAGE_PAGES_VALUE = (size_t)KIMAGE_PAGES;
    pfn_t real_start_pfn = KERNEL_IMAGE_PADDR + KIMAGE_PAGES_VALUE * 0x1000;
    pfn_t aligned_start_pfn = real_start_pfn + 0x200000 - 1;
    aligned_start_pfn &= ~0x1fffff;

    pfn_t saved_start_pfn = aligned_start_pfn;

    memset(physaddr<void>{KERNEL_PD_STRUCT_PAGE_ARR}, 0x00, 4096);

    auto pdpte = KERNEL_PAGE_TABLE[std::get<1>(idx)].parse()[std::get<2>(idx)];
    pdpte.set(PA_KERNEL_PAGE_TABLE, KERNEL_PD_STRUCT_PAGE_ARR);

    auto pd = pdpte.parse();
    for (int i = 0; i < count; ++i, aligned_start_pfn += 0x200000)
        pd[std::get<3>(idx) + i].set(PA_KERNEL_DATA_HUGE, aligned_start_pfn);

    PAGE_ARRAY = (page*)0xffffff8040000000ULL;
    memset(PAGE_ARRAY, 0x00, addr_max * sizeof(page));

    for (int i = 0; i < (int)info::e820_entry_count; ++i) {
        auto& ent = info::e820_entries[i];

        if (ent.type != 1) // type == 1: free area
            continue;
        mark_present(ent.base, ent.base + ent.len);

        auto start = ent.base;
        auto end = start + ent.len;
        if (end <= aligned_start_pfn)
            continue;

        if (start < aligned_start_pfn)
            start = aligned_start_pfn;

        if (start > end)
            continue;

        mem::paging::create_zone(start, end);
    }

    // unused space
    create_zone(0x9000, 0x80000);
    create_zone(0x100000, 0x200000);
    create_zone(real_start_pfn, saved_start_pfn);
}

static inline void save_memory_info(bootloader_data* data) {
    kernel::mem::info::memory_size = 1ULL * 1024ULL * 1024ULL + // initial 1M
                                     1024ULL * data->meminfo_1k_blocks +
                                     64ULL * 1024ULL * data->meminfo_64k_blocks;
    kernel::mem::info::e820_entry_count = data->meminfo_entry_count;
    kernel::mem::info::e820_entry_length = data->meminfo_entry_length;

    memcpy(kernel::mem::info::e820_entries, data->meminfo_entries,
           sizeof(kernel::mem::info::e820_entries));
}

extern "C" void rust_kinit(uintptr_t early_kstack_vaddr);

extern "C" void NORETURN kernel_init(bootloader_data* data) {
    enable_sse();

    setup_early_kernel_page_table();
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

    using namespace mem::paging;
    auto kernel_stack_pfn = page_to_pfn(alloc_pages(9));
    auto kernel_stack_ptr = mem::physaddr<std::byte>{kernel_stack_pfn} + (1 << 9) * 0x1000;

    asm volatile(
        "mov %1, %%rdi\n\t"
        "lea -8(%2), %%rsp\n\t"
        "xor %%rbp, %%rbp\n\t"
        "mov %%rbp, (%%rsp)\n\t" // Clear previous frame pointer
        "jmp *%0\n\t"
        :
        : "r"(rust_kinit), "g"(kernel_stack_pfn), "r"(kernel_stack_ptr)
        : "memory");

    freeze();
}

} // namespace kernel::kinit
