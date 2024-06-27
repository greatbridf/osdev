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

struct PACKED interrupt_stack_head {
    saved_regs s_regs;
    unsigned long int_no;
};

struct PACKED interrupt_stack_normal {
    interrupt_stack_head head;
    uintptr_t v_rip;
    unsigned long cs;
    unsigned long flags;
    uintptr_t rsp;
    unsigned long ss;
};

struct PACKED interrupt_stack_with_code {
    interrupt_stack_head head;
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

// present: When set, the page fault was caused by a page-protection violation.
//          When not set, it was caused by a non-present page.
// write:   When set, the page fault was caused by a write access.
//          When not set, it was caused by a read access.
// user:    When set, the page fault was caused while CPL = 3.
//          This does not necessarily mean that the page fault was a privilege violation.
// from https://wiki.osdev.org/Exceptions#Page_Fault
struct page_fault_error_code {
    unsigned long present : 1;
    unsigned long write : 1;
    unsigned long user : 1;
    unsigned long reserved_write : 1;
    unsigned long instruction_fetch : 1;
    unsigned long protection_key : 1;
    unsigned long shadow_stack : 1;
    unsigned long software_guard_extensions : 1;
};

namespace kernel::kinit {
void init_interrupt();

} // namespace kernel::kinit
