#include <cstddef>

#include <stdint.h>

#include <kernel/mem/phys.hpp>
#include <kernel/mem/types.hpp>
#include <kernel/user/thread_local.hpp>

using namespace kernel::user;

void kernel::user::load_thread_area32(uint64_t desc)
{
    if (!desc)
        return;

    kernel::mem::gdt[7] = desc;

    asm volatile(
        "mov %%gs, %%ax\n\t"
        "mov %%ax, %%gs\n\t"
        : : : "ax"
    );
}
