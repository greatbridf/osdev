#pragma once

#include <kernel/task.h>
#include <types/types.h>

#ifdef __cplusplus
#include <kernel/mm.hpp>
#include <types/list.hpp>

struct process;
struct thread;

struct process_attr {
    uint16_t system : 1;
};

struct thread {
    void* eip;
    process* owner;
};

struct process {
    mm_list mms;
    types::list<thread> thds;
    void* kernel_esp;
    uint16_t kernel_ss;
    process_attr attr;
};

// in process.cpp
extern process* current_process;

extern "C" void NORETURN init_scheduler(tss32_t* tss);

#else

void NORETURN init_scheduler(struct tss32_t* tss);

#endif
