#include <asm/port_io.h>
#include <asm/sys.h>
#include <kernel/interrupt.h>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/syscall.hpp>
#include <kernel/tty.h>
#include <kernel_main.h>
#include <types/allocator.hpp>
#include <types/assert.h>
#include <types/elf.hpp>
#include <types/status.h>
#include <types/stdint.h>

#define SYSCALL_SET_RETURN_VAL_EAX(_eax) \
    data->s_regs.eax = ((decltype(data->s_regs.eax))(_eax))

#define SYSCALL_SET_RETURN_VAL_EDX(_edx) \
    data->s_regs.edx = ((decltype(data->s_regs.edx))(_edx))

#define SYSCALL_SET_RETURN_VAL(_eax, _edx) \
    SYSCALL_SET_RETURN_VAL_EAX(_eax);      \
    SYSCALL_SET_RETURN_VAL_EDX(_edx)

syscall_handler syscall_handlers[8];

void _syscall_not_impl(interrupt_stack* data)
{
    SYSCALL_SET_RETURN_VAL(0xffffffff, 0xffffffff);
}

extern "C" void _syscall_stub_fork_return(void);
void _syscall_fork(interrupt_stack* data)
{
    auto newpid = add_to_process_list(process { *current_process, *current_thread });
    auto* newproc = findproc(newpid);
    thread* newthd = newproc->thds.begin().ptr();
    add_to_ready_list(newthd);

    // create fake interrupt stack
    push_stack(&newthd->esp, data->ss);
    push_stack(&newthd->esp, data->esp);
    push_stack(&newthd->esp, data->eflags);
    push_stack(&newthd->esp, data->cs);
    push_stack(&newthd->esp, (uint32_t)data->v_eip);

    // eax
    push_stack(&newthd->esp, 0);
    push_stack(&newthd->esp, data->s_regs.ecx);
    // edx
    push_stack(&newthd->esp, 0);
    push_stack(&newthd->esp, data->s_regs.ebx);
    push_stack(&newthd->esp, data->s_regs.esp);
    push_stack(&newthd->esp, data->s_regs.ebp);
    push_stack(&newthd->esp, data->s_regs.esi);
    push_stack(&newthd->esp, data->s_regs.edi);

    // ctx_switch stack
    // return address
    push_stack(&newthd->esp, (uint32_t)_syscall_stub_fork_return);
    // ebx
    push_stack(&newthd->esp, 0);
    // edi
    push_stack(&newthd->esp, 0);
    // esi
    push_stack(&newthd->esp, 0);
    // ebp
    push_stack(&newthd->esp, 0);
    // eflags
    push_stack(&newthd->esp, 0);

    SYSCALL_SET_RETURN_VAL(newpid, 0);
}

void _syscall_write(interrupt_stack* data)
{
    tty_print(console, reinterpret_cast<const char*>(data->s_regs.edi));

    SYSCALL_SET_RETURN_VAL(0, 0);
}

void _syscall_sleep(interrupt_stack* data)
{
    current_thread->attr.ready = 0;
    current_thread->attr.wait = 1;

    SYSCALL_SET_RETURN_VAL(0, 0);

    schedule();
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
    const char** argv = reinterpret_cast<const char**>(data->s_regs.esi);

    current_process->mms.clear_user();

    types::elf::elf32_load_data d;
    d.argv = argv;
    d.exec = exec;
    d.system = false;

    assert(types::elf::elf32_load(&d) == GB_OK);

    data->v_eip = d.eip;
    data->esp = (uint32_t)d.sp;
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

    // remove this thread from ready list
    current_thread->attr.ready = 0;
    remove_from_ready_list(current_thread);

    // TODO: write back mmap'ped files and close them

    // unmap all user memory areas
    current_process->mms.clear_user();

    // make child processes orphans (children of init)
    auto children = idx_child_processes->find(current_process->pid);
    if (children) {
        for (auto iter = children->value.begin(); iter != children->value.end(); ++iter)
            findproc(*iter)->ppid = 1;
        idx_child_processes->remove(children);
    }

    // TODO: notify parent process and init

    // switch to new process and continue
    schedule();

    // we should not return to here
    MAKE_BREAK_POINT();
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
