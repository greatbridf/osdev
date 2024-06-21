#include <kernel/process.hpp>
#include <kernel/user/thread_local.hpp>

#include <string.h>
#include <cstddef>

namespace kernel::user {

// TODO: LONG MODE
// void load_thread_area(const segment_descriptor& desc)
// {
//     gdt[6] = desc;
//     asm volatile(
//         "mov %%gs, %%ax\n\t"
//         "mov %%ax, %%gs\n\t"
//         :
//         :
//         : "ax"
//     );
// }

} // namespace kernel::user
