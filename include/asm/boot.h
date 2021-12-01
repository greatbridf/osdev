#pragma once

#include <types/stdint.h>

struct gdt_descriptor {
    uint16_t size;
    uint32_t address;
};

extern struct gdt_descriptor asm_gdt_descriptor;

extern uint32_t check_a20_on(void);

struct e820_mem_map_entry_20 {
    uint64_t base;
    uint64_t len;
    uint32_t type;
};

struct e820_mem_map_entry_24 {
    struct e820_mem_map_entry_20 in;
    uint32_t acpi_extension_attr;
};

extern uint8_t asm_e820_mem_map[1024];
extern uint32_t asm_e820_mem_map_count;
extern uint32_t asm_e820_mem_map_entry_size;

#define e820_mem_map_20 ((struct e820_mem_map_entry_20*)asm_e820_mem_map)
#define e820_mem_map_24 ((struct e820_mem_map_entry_24*)asm_e820_mem_map)
