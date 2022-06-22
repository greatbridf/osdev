#pragma once

#include <types/size.h>
#include <types/stdint.h>

#define PAGE_SIZE (4096)
#define KERNEL_IDENTICALLY_MAPPED_AREA_LIMIT ((void*)0x30000000)

#ifdef __cplusplus
extern "C" {
#endif

// in mem.c
extern struct mm* kernel_mm_head;

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
    linr_ptr_t start;
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
extern size_t kernel_size;
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

// translate physical address to virtual(mapped) address
void* p_ptr_to_v_ptr(phys_ptr_t p_ptr);

// translate linear address to physical address
phys_ptr_t l_ptr_to_p_ptr(struct mm* mm_area, linr_ptr_t v_ptr);

// translate virtual(mapped) address to physical address
phys_ptr_t v_ptr_to_p_ptr(void* v_ptr);

// check if the l_ptr is contained in the area
// @return GB_OK if l_ptr is in the area
//         GB_FAILED if not
int is_l_ptr_valid(struct mm* mm_area, linr_ptr_t l_ptr);

// find the corresponding page the l_ptr pointing to
// @return the pointer to the struct if found, NULL if not found
struct page* find_page_by_l_ptr(struct mm* mm_area, linr_ptr_t l_ptr);

static inline page_t phys_addr_to_page(phys_ptr_t ptr)
{
    return ptr >> 12;
}

static inline pd_i_t page_to_pd_i(page_t p)
{
    return p >> 10;
}

static inline pt_i_t page_to_pt_i(page_t p)
{
    return p & (1024 - 1);
}

static inline phys_ptr_t page_to_phys_addr(page_t p)
{
    return p << 12;
}

static inline pd_i_t linr_addr_to_pd_i(linr_ptr_t ptr)
{
    return page_to_pd_i(phys_addr_to_page(ptr));
}

static inline pd_i_t linr_addr_to_pt_i(linr_ptr_t ptr)
{
    return page_to_pt_i(phys_addr_to_page(ptr));
}

static inline page_directory_entry* lptr_to_pde(struct mm* mm, linr_ptr_t l_ptr)
{
    return mm->pd + linr_addr_to_pd_i((phys_ptr_t)l_ptr);
}

static inline page_table_entry* lptr_to_pte(struct mm* mm, linr_ptr_t l_ptr)
{
    page_directory_entry* pde = lptr_to_pde(mm, l_ptr);
    page_table_entry* pte = (page_table_entry*)p_ptr_to_v_ptr(page_to_phys_addr(pde->in.pt_page));
    return pte + linr_addr_to_pt_i((phys_ptr_t)l_ptr);
}

static inline page_directory_entry* lp_to_pde(struct mm* mm, linr_ptr_t l_ptr)
{
    phys_ptr_t p_ptr = l_ptr_to_p_ptr(mm, l_ptr);
    page_directory_entry* pde = mm->pd + linr_addr_to_pd_i(p_ptr);
    return pde;
}

// get the corresponding pte for the linear address
// for example: l_ptr = 0x30001000 will return the pte including the page it is mapped to
static inline page_table_entry* lp_to_pte(struct mm* mm, linr_ptr_t l_ptr)
{
    phys_ptr_t p_ptr = l_ptr_to_p_ptr(mm, l_ptr);

    page_directory_entry* pde = lp_to_pde(mm, l_ptr);
    phys_ptr_t p_pt = page_to_phys_addr(pde->in.pt_page);

    page_table_entry* pte = (page_table_entry*)p_ptr_to_v_ptr(p_pt);
    pte += linr_addr_to_pt_i(p_ptr);

    return pte;
}

// map the page to the end of the mm_area in pd
int k_map(
    struct mm* mm_area,
    struct page* page,
    int read,
    int write,
    int priv,
    int cow);

// allocate a raw page
page_t alloc_raw_page(void);

// allocate a struct page together with the raw page
struct page* allocate_page(void);

#define KERNEL_PAGE_DIRECTORY_ADDR ((page_directory_entry*)0x00000000)

void init_mem(void);

#define KERNEL_CODE_SEGMENT (0x08)
#define KERNEL_DATA_SEGMENT (0x10)
#define USER_CODE_SEGMENT (0x18)
#define USER_DATA_SEGMENT (0x20)

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
