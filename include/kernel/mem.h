#pragma once

#include <stdint.h>
#include <types/size.h>

#define PAGE_SIZE (4096)
#define IDENTICALLY_MAPPED_HEAP_SIZE ((size_t)0x400000)
#define KERNEL_IDENTICALLY_MAPPED_AREA_LIMIT ((void*)0x30000000)

#ifdef __cplusplus
extern "C" {
#endif

// don't forget to add the initial 1m to the total
struct mem_size_info {
    uint16_t n_1k_blks; // memory between 1m and 16m in 1k blocks
    uint16_t n_64k_blks; // memory above 16m in 64k blocks
};

struct e820_mem_map_entry_20 {
    uint64_t base;
    uint64_t len;
    uint32_t type;
};

struct e820_mem_map_entry_24 {
    struct e820_mem_map_entry_20 in;
    uint32_t acpi_extension_attr;
};

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
typedef union pde_t {
    uint32_t v;
    struct {
        uint32_t p : 1;
        uint32_t rw : 1;
        uint32_t us : 1;
        uint32_t pwt : 1;
        uint32_t pcd : 1;
        uint32_t a : 1;
        uint32_t d : 1;
        uint32_t ps : 1;
        uint32_t ignored : 4;
        page_t pt_page : 20;
    } in;
} pde_t;
typedef pde_t (*pd_t)[1024];

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
typedef union pte_t {
    uint32_t v;
    struct {
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
        page_t page : 20;
    } in;
} pte_t;
typedef pte_t (*pt_t)[1024];

// in mem.cpp
extern uint8_t e820_mem_map[1024];
extern uint32_t e820_mem_map_count;
extern uint32_t e820_mem_map_entry_size;
extern struct mem_size_info mem_size_info;

#define KERNEL_HEAP_START ((void*)0x30000000)
#define KERNEL_HEAP_LIMIT ((void*)0x40000000)

void* k_malloc(size_t size);

void k_free(void* ptr);

void* ki_malloc(size_t size);

void ki_free(void* ptr);

#define KERNEL_PAGE_DIRECTORY_ADDR ((pd_t)0x00001000)

void init_mem(void);

#define KERNEL_CODE_SEGMENT (0x08)
#define KERNEL_DATA_SEGMENT (0x10)
#define USER_CODE_SEGMENT (0x18)
#define USER_DATA_SEGMENT (0x20)
#define USER_CODE_SELECTOR (USER_CODE_SEGMENT | 3)
#define USER_DATA_SELECTOR (USER_DATA_SEGMENT | 3)

#define SD_TYPE_CODE_SYSTEM (0x9a)
#define SD_TYPE_DATA_SYSTEM (0x92)

#define SD_TYPE_CODE_USER (0xfa)
#define SD_TYPE_DATA_USER (0xf2)

#define SD_TYPE_TSS (0x89)

typedef struct segment_descriptor_struct {
    uint64_t limit_low : 16;
    uint64_t base_low : 16;
    uint64_t base_mid : 8;
    uint64_t access : 8;
    uint64_t limit_high : 4;
    uint64_t flags : 4;
    uint64_t base_high : 8;
} segment_descriptor;

// in mem.cpp
extern segment_descriptor gdt[6];

void create_segment_descriptor(
    segment_descriptor* sd,
    uint32_t base,
    uint32_t limit,
    uint32_t flags,
    uint32_t access);

#ifdef __cplusplus
}
#endif
