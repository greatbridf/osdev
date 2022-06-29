#pragma once

#include <types/types.h>
#include <kernel/interrupt.h>

// return value is stored in %eax and %edx
typedef void (*syscall_handler)(interrupt_stack* data);

void init_syscall(void);
