#include <types/stdint.h>

struct gdt_descriptor {
    uint16_t size;
    uint32_t address;
};

extern struct gdt_descriptor asm_gdt_descriptor;
