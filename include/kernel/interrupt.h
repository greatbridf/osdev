#pragma once

#include <types/types.h>

#ifdef __cplusplus
extern "C" {
#endif

#define KERNEL_INTERRUPT_GATE_TYPE (0x8e)
#define USER_INTERRUPT_GATE_TYPE (0xee)

#define PIC_EOI (0x20)

struct regs_32 {
    uint32_t edi;
    uint32_t esi;
    uint32_t ebp;
    uint32_t esp;
    uint32_t ebx;
    uint32_t edx;
    uint32_t ecx;
    uint32_t eax;
};

struct interrupt_stack {
    struct regs_32 s_regs;
    void* v_eip;
    uint32_t cs;
    uint32_t eflags;
    uint32_t esp;
    uint32_t ss;
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
