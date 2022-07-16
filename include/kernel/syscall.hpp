#pragma once

#include <kernel/interrupt.h>
#include <types/types.h>

// return value is stored in %eax and %edx
typedef void (*syscall_handler)(interrupt_stack* data);

inline uint32_t syscall(uint32_t num, uint32_t arg1 = 0, uint32_t arg2 = 0)
{
    asm volatile(
        "movl %1, %%edi\n"
        "movl %2, %%esi\n"
        "movl %3, %%eax\n"
        "int $0x80\n"
        "movl %%eax, %0"
        : "=g"(num)
        : "g"(arg1), "g"(arg2), "g"(num)
        : "eax", "edx", "edi", "esi");
    return num;
}

void init_syscall(void);
