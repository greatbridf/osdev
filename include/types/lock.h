#pragma once

#include <types/stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

static inline void spin_lock(uint32_t* lock_addr)
{
    asm volatile(
            "_spin:\n\t\
             movl $1, %%eax\n\t\
             xchgl %%eax, (%0)\n\t\
             test $0, %%eax\n\t\
             jne _spin\n\t\
            "
            : "=r" (lock_addr)
            : "0"  (lock_addr)
            : "eax", "memory"
            );
}

static inline void spin_unlock(uint32_t* lock_addr)
{
    asm volatile("movl $0, %%eax\nxchgl %%eax, (%0)"
                 :
                 : "r"  (lock_addr)
                 : "eax", "memory"
                 );
}

#ifdef __cplusplus
}
#endif
