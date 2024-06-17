#pragma once

#include <types/types.h>

#ifdef __cplusplus
extern "C" {
#endif

#define KERNEL_INTERRUPT_GATE_TYPE (0x8e)
#define USER_INTERRUPT_GATE_TYPE (0xee)

#define PIC_EOI (0x20)

struct regs_64 {
    uint64_t rax;
    uint64_t rbx;
    uint64_t rcx;
    uint64_t rdx;
    uint64_t rsi;
    uint64_t rdi;
    uint64_t rsp;
    uint64_t rbp;
    uint64_t r8;
    uint64_t r9;
    uint64_t r10;
    uint64_t r11;
    uint64_t r12;
    uint64_t r13;
    uint64_t r14;
    uint64_t r15;
};

struct interrupt_stack {
    regs_64 s_regs;
    void* v_rip;
    uint64_t cs;
    uint64_t flags;
    uint64_t rsp;
    uint64_t ss;
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
    uint32_t present : 1;
    uint32_t write : 1;
    uint32_t user : 1;
    uint32_t reserved_write : 1;
    uint32_t instruction_fetch : 1;
    uint32_t protection_key : 1;
    uint32_t shadow_stack : 1;
    uint32_t software_guard_extensions : 1;
};

void init_idt(void);
void init_pic(void);

#ifdef __cplusplus
}
#endif
