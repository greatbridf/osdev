#pragma once

#include <types/types.h>
#include <kernel/interrupt.h>

struct PACKED syscall_stack_data {
    struct regs_32 s_regs;
    void* v_eip;
    uint32_t cs;
    uint32_t eflags;
    uint32_t esp;
    uint32_t ss;
};

// return value is stored in %eax and %edx
typedef void (*syscall_handler)(syscall_stack_data* data);

void init_syscall(void);
