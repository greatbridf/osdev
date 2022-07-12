#pragma once

#include <types/types.h>
#include <kernel/interrupt.h>

// return value is stored in %eax and %edx
typedef void (*syscall_handler)(interrupt_stack* data);

#define syscall(eax) asm volatile("movl %0, %%eax\n\tint $0x80"::"r"(eax):"eax","edx")

void init_syscall(void);
