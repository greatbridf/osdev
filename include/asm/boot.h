#pragma once

#include <types/stdint.h>

#define KERNEL_EARLY_STACK_ADDR ((phys_ptr_t)0x01000000)
#define KERNEL_EARLY_STACK_SIZE ((size_t)0x100000)

struct __attribute__((__packed__)) gdt_descriptor {
    uint16_t size;
    uint32_t address;
};

extern struct gdt_descriptor asm_gdt_descriptor;

extern struct mem_size_info asm_mem_size_info;

extern uint8_t asm_e820_mem_map[1024];
extern uint32_t asm_e820_mem_map_count;
extern uint32_t asm_e820_mem_map_entry_size;

extern uint32_t asm_kernel_size;

extern uint32_t bss_section_start_addr;
extern uint32_t bss_section_end_addr;
