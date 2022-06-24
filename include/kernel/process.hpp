#pragma once

#include <kernel/interrupt.h>
#include <kernel/task.h>
#include <types/types.h>

#ifdef __cplusplus
#include <kernel/mm.hpp>
#include <types/list.hpp>

class process;
struct thread;

struct process_attr {
    uint16_t system : 1;
};

struct thread {
    void* eip;
    process* owner;
    regs_32 regs;
    uint32_t eflags;
    uint32_t esp;
};

class process {
public:
    mm_list mms;
    types::list<thread> thds;
    void* k_esp;
    process_attr attr;

public:
    process(process&& val);
    process(const process&) = delete;
    process(void* start_eip, uint8_t* image, size_t image_size, bool system);
};

// in process.cpp
extern process* current_process;

extern "C" void NORETURN init_scheduler();
void context_switch(irq0_data* intrpt_data);

#else

void NORETURN init_scheduler();

#endif
