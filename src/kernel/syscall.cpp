#include <asm/port_io.h>
#include <kernel/interrupt.h>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/syscall.hpp>
#include <kernel/tty.h>
#include <types/allocator.hpp>
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

    unmap_user_space_memory(current_process->mms);

    types::elf::elf32_load(exec, data, current_process->attr.system);
}

// @param exit_code
void _syscall_exit(interrupt_stack* data)
{
    uint32_t exit_code = data->s_regs.edi;

    // TODO: terminating a thread only
    if (current_thread->owner->thds.size() != 1) {
        _syscall_crash(data);
    }

    // terminating a whole process:

    // clear threads
    remove_from_ready_list(current_thread);
    current_process->thds.clear();

    // TODO: write back mmap'ped files and close them

    // unmap all memory areas
    auto& mms = current_process->mms;

    unmap_user_space_memory(mms);

    pd_t old_pd = mms.begin()->pd;
    current_process->mms.clear();

    // make child processes orphans (children of init)
    auto children = idx_child_processes->find(current_process->pid);
    if (children) {
        for (auto iter = children->value.begin(); iter != children->value.end(); ++iter)
            findproc(*iter)->ppid = 1;
        idx_child_processes->remove(children);
    }

    // TODO: notify parent process and init

    // switch to new process and continue
    auto iter_next_thd = query_next_thread();
    auto* next_thd = *iter_next_thd;

    process_context_load(data, next_thd->owner);
    thread_context_load(data, next_thd);

    next_task(iter_next_thd);

    // destroy page directory
    dealloc_pd(old_pd);

    context_jump(next_thd->attr.system, data);
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
