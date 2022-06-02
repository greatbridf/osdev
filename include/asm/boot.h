#pragma once

#include <types/stdint.h>

struct __attribute__((__packed__)) gdt_descriptor {
    uint16_t size;
    uint32_t address;
};

extern struct gdt_descriptor asm_gdt_descriptor;

extern uint32_t check_a20_on(void);

extern struct mem_size_info asm_mem_size_info;

extern uint8_t asm_e820_mem_map[1024];
extern uint32_t asm_e820_mem_map_count;
extern uint32_t asm_e820_mem_map_entry_size;

extern uint32_t asm_kernel_size;
