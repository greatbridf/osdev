#pragma once

#include <types/stdint.h>

struct mem_size_info {
    uint16_t n_16k_blks;
    uint16_t n_64k_blks;
};

extern struct mem_size_info asm_mem_size_info;
