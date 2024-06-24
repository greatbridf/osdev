#pragma once

#include <types/types.h>

#include <kernel/interrupt.hpp>

#define SYSCALL_ARG1(type, name) type name = (type)((data)->head.s_regs.rdi)
#define SYSCALL_ARG2(type, name) type name = (type)((data)->head.s_regs.rsi)
#define SYSCALL_ARG3(type, name) type name = (type)((data)->head.s_regs.rdx)
#define SYSCALL_ARG4(type, name) type name = (type)((data)->head.s_regs.r10)
#define SYSCALL_ARG5(type, name) type name = (type)((data)->head.s_regs.r8)
#define SYSCALL_ARG6(type, name) type name = (type)((data)->head.s_regs.r9)

// return value is stored in %rax
typedef long (*syscall_handler)(interrupt_stack_normal* data);

void init_syscall(void);
