#pragma once

#include <kernel/interrupt.h>
#include <types/types.h>

// return value is stored in %eax and %edx
typedef int (*syscall_handler)(interrupt_stack* data);

void init_syscall(void);
