#include <asm/port_io.h>
#include <kernel/interrupt.h>
#include <kernel/process.hpp>
#include <kernel/syscall.hpp>
#include <kernel/tty.h>
#include <types/elf.hpp>

syscall_handler syscall_handlers[8];

void _syscall_not_impl(interrupt_stack* data)
{
    data->s_regs.eax = 0xffffffff;
    data->s_regs.edx = 0xffffffff;
}

void _syscall_fork(interrupt_stack* data)
{
    thread_context_save(data, current_thread);
    process_context_save(data, current_process);

    process new_proc(*current_process, *current_thread);
    thread* new_thd = new_proc.thds.begin().ptr();

    // return value
    new_thd->regs.eax = 0;
    data->s_regs.eax = new_proc.pid;

    new_thd->regs.edx = 0;
    data->s_regs.edx = 0;

    add_to_process_list(types::move(new_proc));
    add_to_ready_list(new_thd);
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

    data->s_regs.eax = 0;
    data->s_regs.edx = 0;

    do_scheduling(data);
}

void _syscall_crash(interrupt_stack*)
{
    tty_print(console, "\nan error occurred while executing command\n");
    asm_cli();
    asm_hlt();
}

// syscall_exec(const char* exec, const char** argv)
// @param exec: the path of program to execute
// @param argv: arguments end with nullptr
void _syscall_exec(interrupt_stack* data)
{
    const char* exec = reinterpret_cast<const char*>(data->s_regs.edi);

    // TODO: load argv
    const char** argv = reinterpret_cast<const char**>(data->s_regs.esi);
    (void)argv;

    // skip kernel heap
    for (auto iter = ++current_process->mms.begin(); iter != current_process->mms.end();) {
        k_unmap(iter.ptr());
        iter = current_process->mms.erase(iter);
    }

    types::elf::elf32_load(exec, data, current_process->attr.system);
}

void _syscall_exit(interrupt_stack* data)
{
    _syscall_crash(data);
}

void init_syscall(void)
{
    syscall_handlers[0] = _syscall_fork;
    syscall_handlers[1] = _syscall_write;
    syscall_handlers[2] = _syscall_sleep;
    syscall_handlers[3] = _syscall_crash;
    syscall_handlers[4] = _syscall_exec;
    syscall_handlers[5] = _syscall_exit;
    syscall_handlers[6] = _syscall_not_impl;
    syscall_handlers[7] = _syscall_not_impl;
}
