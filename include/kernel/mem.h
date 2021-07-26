#pragma once

#include <types/stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// don't forget to add the initial 1m to the total
struct mem_size_info {
    uint16_t n_1k_blks; // memory between 1m and 16m in 1k blocks
    uint16_t n_64k_blks; // memory above 16m in 64k blocks
};

extern struct mem_size_info asm_mem_size_info;

// TODO: decide heap start address according
//   to user's memory size
#define HEAP_START ((void*)0x01000000)

struct mem_blk_flags {
    uint8_t is_free;
    uint8_t has_next;
    uint8_t _unused2;
    uint8_t _unused3;
};

struct mem_blk {
    size_t size;
    struct mem_blk_flags flags;
    // the first byte of the memory space
    // the minimal allocated space is 4 bytes
    uint8_t data[4];
};

void init_heap(void);

void* k_malloc(size_t size);

void k_free(void* ptr);

#ifdef __cplusplus
}
#endif
