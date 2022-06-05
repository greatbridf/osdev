#pragma once

#include <types/size.h>
#include <types/stdint.h>

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
    page_t pt_page : 20;
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
    page_t page : 20;
};

typedef union page_table_entry {
    uint32_t v;
    struct page_table_entry_in in;
} page_table_entry;

struct page_attr {
    uint32_t read : 1;
    uint32_t write : 1;
    uint32_t system : 1;
    uint32_t cow : 1;
};

struct page {
    page_t phys_page_id;
    size_t* ref_count;
    struct page_attr attr;
    struct page* next;
};

struct mm_attr {
    uint32_t read : 1;
    uint32_t write : 1;
    uint32_t system : 1;
};

struct mm {
    void* start;
    size_t len;
    struct mm_attr attr;
    struct page* pgs;
    struct mm* next;
    page_directory_entry* pd;
};

// in kernel_main.c
extern uint8_t e820_mem_map[1024];
extern uint32_t e820_mem_map_count;
extern uint32_t e820_mem_map_entry_size;
extern uint32_t kernel_size;
extern struct mem_size_info mem_size_info;

#define KERNEL_HEAP_START ((void*)0x30000000)
#define KERNEL_HEAP_LIMIT ((void*)0x40000000)

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

int init_heap(void);

void* k_malloc(size_t size);

void k_free(void* ptr);

// translate physical address to linear address
void* p_ptr_to_v_ptr(phys_ptr_t p_ptr);

// translate linear address to physical address
phys_ptr_t v_ptr_to_p_ptr(struct mm* mm_area, void* v_ptr);

#define KERNEL_PAGE_DIRECTORY_ADDR ((page_directory_entry*)0x00000000)

void init_mem(void);

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

void create_segment_descriptor(
    segment_descriptor* sd,
    uint32_t base,
    uint32_t limit,
    uint32_t flags,
    uint32_t access);

#ifdef __cplusplus
}
#endif
