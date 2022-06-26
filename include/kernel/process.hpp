#pragma once

#include <kernel/interrupt.h>
#include <kernel/task.h>
#include <types/types.h>

#ifdef __cplusplus
#include <kernel/mm.hpp>
#include <types/list.hpp>

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
    uint32_t esp;
    thread_attr attr;
};

class process {
public:
    mm_list mms;
    types::list<thread> thds;
    void* k_esp;
    process_attr attr;
    pid_t pid;

public:
    process(process&& val);
    process(const process&) = delete;
    process(const process& proc, const thread& main_thread);
    process(void* start_eip, uint8_t* image, size_t image_size, bool system);
};

// in process.cpp
extern process* current_process;
extern thread* current_thread;

extern "C" void NORETURN init_scheduler();
void do_scheduling(interrupt_stack* intrpt_data);

void thread_context_save(interrupt_stack* int_stack, thread* thd, bool kernel);
void thread_context_load(interrupt_stack* int_stack, thread* thd, bool kernel);
void process_context_save(interrupt_stack*, process*);
void process_context_load(interrupt_stack*, process* proc);

void add_to_process_list(process&& proc);
void add_to_ready_list(thread* thd);

#else

void NORETURN init_scheduler();

#endif
