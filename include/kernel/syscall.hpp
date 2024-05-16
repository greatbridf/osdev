#pragma once

#include <kernel/interrupt.h>
#include <types/types.h>

#define SYSCALL_ARG1(type, name) type name = (type)((data)->s_regs.ebx)
#define SYSCALL_ARG2(type, name) type name = (type)((data)->s_regs.ecx)
#define SYSCALL_ARG3(type, name) type name = (type)((data)->s_regs.edx)
#define SYSCALL_ARG4(type, name) type name = (type)((data)->s_regs.esi)
#define SYSCALL_ARG5(type, name) type name = (type)((data)->s_regs.edi)
#define SYSCALL_ARG6(type, name) type name = (type)((data)->s_regs.ebp)

// return value is stored in %eax and %edx
typedef int (*syscall_handler)(interrupt_stack* data);

void init_syscall(void);
