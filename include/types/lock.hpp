#pragma once

#include <types/stdint.h>

inline void spin_lock(uint32_t volatile* lock_addr)
{
    asm volatile(
        "0:\n\t\
         movl $1, %%eax\n\t\
         xchgl %%eax, (%0)\n\t\
         test $0, %%eax\n\t\
         jne 0\n\t\
        "
        :
        : "r"(lock_addr)
        : "eax", "memory");
}

inline void spin_unlock(uint32_t volatile* lock_addr)
{
    asm volatile(
        "movl $0, %%eax\n\
         xchgl %%eax, (%0)"
        :
        : "r"(lock_addr)
        : "eax", "memory");
}
