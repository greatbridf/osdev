#include <kernel/syscall.hpp>

syscall_handler syscall_handlers[8];

void _syscall_not_impl(syscall_stack_data* data)
{
    data->s_regs.eax = 0xffffffff;
    data->s_regs.edx = 0xffffffff;
}

void _syscall_fork(syscall_stack_data* data)
{
    data->s_regs.eax = 0xfafafafa;
    data->s_regs.edx = 0xfefefefe;
}

void init_syscall(void)
{
    syscall_handlers[0] = _syscall_fork;
    syscall_handlers[1] = _syscall_not_impl;
    syscall_handlers[2] = _syscall_not_impl;
    syscall_handlers[3] = _syscall_not_impl;
    syscall_handlers[4] = _syscall_not_impl;
    syscall_handlers[5] = _syscall_not_impl;
    syscall_handlers[6] = _syscall_not_impl;
    syscall_handlers[7] = _syscall_not_impl;
}
