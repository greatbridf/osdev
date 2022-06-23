#pragma once

#include <kernel/task.h>
#include <types/types.h>

#ifdef __cplusplus
#include <kernel/mm.hpp>

struct process {
    mm_list* mms;
    void* kernel_esp;
    void* eip;
    uint16_t kernel_ss;
    uint16_t cs;
};

// in process.cpp
extern process* current_process;

extern "C" void NORETURN init_scheduler(tss32_t* tss);

#else

void NORETURN init_scheduler(struct tss32_t* tss);

#endif
