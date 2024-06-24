#include <cstddef>

#include <stdint.h>

#include <kernel/mem/phys.hpp>
#include <kernel/mem/types.hpp>
#include <kernel/user/thread_local.hpp>

using namespace kernel::user;

void kernel::user::load_thread_area32(uint64_t desc)
{
    mem::gdt[7] = desc;
    asm volatile(
        "mov %%gs, %%ax\n\t"
        "mov %%ax, %%gs\n\t"
        : : : "ax"
    );
}

void kernel::user::load_thread_area64(uint64_t desc_lo, uint64_t desc_hi)
{
    mem::gdt[12] = desc_lo;
    mem::gdt[13] = desc_hi;

    asm volatile(
        "mov %%fs, %%ax\n\t"
        "mov %%ax, %%fs\n\t"
        "mov %%gs, %%ax\n\t"
        "mov %%ax, %%gs\n\t"
        : : : "ax"
    );
}

void kernel::user::load_thread_area(uint64_t desc_lo, uint64_t desc_hi)
{
    load_thread_area64(desc_lo, desc_hi);
}
