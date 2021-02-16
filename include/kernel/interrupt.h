#pragma once

#include <types/types.h>

#define INTERRUPT_GATE_TYPE (0x8e)

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

// external interrupt handler function
// stub in assembly MUST be called irqN
#define SET_UP_IRQ(N, SELECTOR)        \
    extern void irq##N();              \
    ptr_t addr_irq##N = (ptr_t)irq##N; \
    SET_IDT_ENTRY(0x20 + (N), (addr_irq##N), (SELECTOR));

#define SET_IDT_ENTRY_FN(N, FUNC_NAME, SELECTOR) \
    extern void FUNC_NAME();                     \
    ptr_t addr_##FUNC_NAME = (ptr_t)FUNC_NAME;   \
    SET_IDT_ENTRY((N), (addr_##FUNC_NAME), (SELECTOR));

#define SET_IDT_ENTRY(N, ADDR, SELECTOR)      \
    IDT[(N)].offset_low = (ADDR)&0x0000ffff;  \
    IDT[(N)].selector = (SELECTOR);           \
    IDT[(N)].zero = 0;                        \
    IDT[(N)].type_attr = INTERRUPT_GATE_TYPE; \
    IDT[(N)].offset_high = ((ADDR)&0xffff0000) >> 16

struct IDT_entry {
    uint16_t offset_low;
    uint16_t selector;
    uint8_t zero;
    uint8_t type_attr;
    uint16_t offset_high;
};

#ifndef _INTERRUPT_C_
extern struct IDT_entry IDT[256];
#endif

void init_idt();

// idt_descriptor: uint16_t[3]
// [0] bit 0 :15 => limit
// [1] bit 16:47 => address
extern void asm_load_idt(uint16_t idt_descriptor[3]);

void int13_handler(
    struct regs_32 s_regs,
    uint32_t error_code,
    ptr_t eip,
    uint16_t cs,
    uint32_t eflags);

void irq0_handler(void);
void irq1_handler(void);
void irq2_handler(void);
void irq3_handler(void);
void irq4_handler(void);
void irq5_handler(void);
void irq6_handler(void);
void irq7_handler(void);
void irq8_handler(void);
void irq9_handler(void);
void irq10_handler(void);
void irq11_handler(void);
void irq12_handler(void);
void irq13_handler(void);
void irq14_handler(void);
void irq15_handler(void);
