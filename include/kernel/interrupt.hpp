#pragma once

#include <stdint.h>

#include <types/types.h>

struct saved_regs {
    unsigned long rax;
    unsigned long rbx;
    unsigned long rcx;
    unsigned long rdx;
    unsigned long rdi;
    unsigned long rsi;
    unsigned long r8;
    unsigned long r9;
    unsigned long r10;
    unsigned long r11;
    unsigned long r12;
    unsigned long r13;
    unsigned long r14;
    unsigned long r15;
    unsigned long rbp;
};

struct interrupt_stack {
    saved_regs regs;
    unsigned long int_no;
    unsigned long error_code;
    uintptr_t v_rip;
    unsigned long cs;
    unsigned long flags;
    uintptr_t rsp;
    unsigned long ss;
};

struct mmx_registers {
    uint8_t data[512]; // TODO: list of content
};

namespace kernel::kinit {
void init_interrupt();

} // namespace kernel::kinit
