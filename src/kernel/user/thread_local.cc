#include <kernel/mem.h>
#include <kernel/user/thread_local.hpp>

#include <string.h>
#include <cstddef>

namespace kernel::user {

int set_thread_area(user_desc* ptr)
{
    if (ptr->read_exec_only && ptr->seg_not_present) {
        void* dst = (void*)ptr->base_addr;
        std::size_t len = ptr->limit;
        if (len > 0 && dst)
            memset(dst, 0x00, len);
        return 0;
    }

    if (ptr->entry_number == -1U)
        ptr->entry_number = 6;
    else
        return -1;

    gdt[6].limit_low = ptr->limit & 0xFFFF;
    gdt[6].base_low = ptr->base_addr & 0xFFFF;
    gdt[6].base_mid = (ptr->base_addr >> 16) & 0xFF;
    gdt[6].access = SD_TYPE_DATA_USER;
    gdt[6].limit_high = (ptr->limit >> 16) & 0xF;
    gdt[6].flags = (ptr->limit_in_pages << 3) | (ptr->seg_32bit << 2);
    gdt[6].base_high = (ptr->base_addr >> 24) & 0xFF;

    return 0;
}

} // namespace kernel::user
