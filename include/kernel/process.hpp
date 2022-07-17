#pragma once

#include <kernel/interrupt.h>
#include <kernel/mm.hpp>
#include <kernel/task.h>
#include <types/hash_map.hpp>
#include <types/list.hpp>
#include <types/types.h>

typedef size_t pid_t;

class process;
struct thread;

struct process_attr {
    uint16_t system : 1;
};

struct thread_attr {
    uint32_t system : 1;
    uint32_t ready : 1;
    uint32_t wait : 1;
};

struct thread {
    void* eip;
    process* owner;
    regs_32 regs;
    uint32_t eflags;
    thread_attr attr;
};

class process {
public:
    mm_list mms;
    types::list<thread> thds;
    // TODO: allocate a kernel stack for EVERY THREAD
    void* k_esp;
    process_attr attr;
    pid_t pid;
    pid_t ppid;

public:
    process(process&& val);
    process(const process&) = delete;
    process(const process& proc, const thread& main_thread);

    // only used for system initialization
    process(void* start_eip);
};

inline process* volatile current_process;
inline thread* volatile current_thread;
inline typename types::hash_map<pid_t, types::list<pid_t>, types::linux_hasher<pid_t>>* idx_child_processes;

extern "C" void NORETURN init_scheduler();
void do_scheduling(interrupt_stack* intrpt_data);

void thread_context_save(interrupt_stack* int_stack, thread* thd);
void thread_context_load(interrupt_stack* int_stack, thread* thd);
void process_context_save(interrupt_stack*, process*);
void process_context_load(interrupt_stack*, process* proc);

void add_to_process_list(process&& proc);

void add_to_ready_list(thread* thd);
void remove_from_ready_list(thread* thd);
types::list<thread*>::iterator_type query_next_thread(void);

// the function call INVALIDATES iterator
inline void next_task(types::list<thread*>::iterator_type target)
{
    auto* ptr = *target;
    remove_from_ready_list(ptr);
    if (ptr->attr.ready)
        add_to_ready_list(ptr);
}

extern "C" void NORETURN to_kernel(interrupt_stack* ret_stack);
extern "C" void NORETURN to_user(interrupt_stack* ret_stack);

inline void NORETURN context_jump(bool system, interrupt_stack* intrpt_stack)
{
    if (system)
        to_kernel(intrpt_stack);
    else
        to_user(intrpt_stack);
}

process* findproc(pid_t pid);

void k_new_thread(void (*func)(void*), void* data);
