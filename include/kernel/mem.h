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

/*
 * page directory entry
 *
 * p   : present (1)
 * rw  : allow write (1)
 * us  : allow user access (1)
 * pwt : todo
 * pcd : todo
 * a   : accessed for linear address translation (1)
 * d   : dirty (1) (ignored)
 * ps  : use 4MiB pages (ignored)
 * addr: page table address
 */
struct page_directory_entry_in {
    uint32_t p : 1;
    uint32_t rw : 1;
    uint32_t us : 1;
    uint32_t pwt : 1;
    uint32_t pcd : 1;
    uint32_t a : 1;
    uint32_t d : 1;
    uint32_t ps : 1;
    uint32_t ignored : 4;
    uint32_t addr : 20;
};

typedef union page_directory_entry {
    uint32_t v;
    struct page_directory_entry_in in;
} page_directory_entry;

/*
 * page table entry
 *
 * p   : present (1)
 * rw  : allow write (1)
 * us  : allow user access (1)
 * pwt : todo
 * pcd : todo
 * a   : accessed for linear address translation (1)
 * d   : dirty (1)
 * pat : todo (ignored)
 * g   : used in cr4 mode (ignored)
 * addr: physical memory address
 */
struct page_table_entry_in {
    uint32_t p : 1;
    uint32_t rw : 1;
    uint32_t us : 1;
    uint32_t pwt : 1;
    uint32_t pcd : 1;
    uint32_t a : 1;
    uint32_t d : 1;
    uint32_t pat : 1;
    uint32_t g : 1;
    uint32_t ignored : 3;
    uint32_t addr : 20;
};

typedef union page_table_entry {
    uint32_t v;
    struct page_table_entry_in in;
} page_table_entry;

#define KERNEL_PAGE_DIRECTORY_ADDR ((page_directory_entry*)0x00000000)
#define KERNEL_PAGE_TABLE_START_ADDR ((page_table_entry*)0x00100000)

void init_paging(void);

#ifdef __cplusplus
}
#endif
