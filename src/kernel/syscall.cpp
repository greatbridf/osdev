#include <asm/port_io.h>
#include <kernel/syscall.hpp>
#include <kernel/process.hpp>
#include <kernel/tty.h>

syscall_handler syscall_handlers[8];

void _syscall_not_impl(interrupt_stack* data)
{
    data->s_regs.eax = 0xffffffff;
    data->s_regs.edx = 0xffffffff;
}

void _syscall_fork(interrupt_stack* data)
{
    data->s_regs.eax = 0xfafafafa;
    data->s_regs.edx = 0xfefefefe;
}

void _syscall_write(interrupt_stack* data)
{
    tty_print(console, reinterpret_cast<const char*>(data->s_regs.edi));
    data->s_regs.eax = 0;
    data->s_regs.edx = 0;
}

void _syscall_sleep(interrupt_stack* data)
{
    current_thread->attr.ready = 0;
    current_thread->attr.wait = 1;
    do_scheduling(data);
}

void init_syscall(void)
{
    syscall_handlers[0] = _syscall_fork;
    syscall_handlers[1] = _syscall_write;
    syscall_handlers[2] = _syscall_sleep;
    syscall_handlers[3] = _syscall_not_impl;
    syscall_handlers[4] = _syscall_not_impl;
    syscall_handlers[5] = _syscall_not_impl;
    syscall_handlers[6] = _syscall_not_impl;
    syscall_handlers[7] = _syscall_not_impl;
}
