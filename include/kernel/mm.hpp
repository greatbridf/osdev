#pragma once

#include <kernel/mem.h>
#include <kernel/vfs.hpp>
#include <types/allocator.hpp>
#include <types/list.hpp>
#include <types/types.h>
#include <types/vector.hpp>

constexpr size_t THREAD_KERNEL_STACK_SIZE = 2 * PAGE_SIZE;

struct page_attr {
    uint32_t cow : 1;
};

struct page {
    page_t phys_page_id;
    size_t* ref_count;
    struct page_attr attr;
};

using page_arr = types::vector<page, types::kernel_ident_allocator>;

struct mm_attr {
    uint32_t read : 1;
    uint32_t write : 1;
    uint32_t system : 1;
};

class mm {
public:
    linr_ptr_t start;
    struct mm_attr attr;
    page_directory_entry* pd;
    page_arr* pgs;
    fs::inode* mapped_file;
    size_t file_offset;

public:
    mm(const mm& val);
    mm(linr_ptr_t start, page_directory_entry* pd, bool write, bool system);
};

using mm_list = types::list<mm, types::kernel_ident_allocator>;

// in mem.cpp
extern mm_list* kernel_mms;
extern page empty_page;

// translate physical address to virtual(mapped) address
void* p_ptr_to_v_ptr(phys_ptr_t p_ptr);

// translate linear address to physical address
phys_ptr_t l_ptr_to_p_ptr(const mm_list* mms, linr_ptr_t v_ptr);

// translate virtual(mapped) address to physical address
phys_ptr_t v_ptr_to_p_ptr(void* v_ptr);

// @return the pointer to the mm_area containing l_ptr
//         nullptr if not
mm* find_mm_area(mm_list* mms, linr_ptr_t l_ptr);

// find the corresponding page the l_ptr pointing to
// @return the pointer to the struct if found, NULL if not found
struct page* find_page_by_l_ptr(const mm_list* mms, linr_ptr_t l_ptr);

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

static inline page_directory_entry* mms_get_pd(const mm_list* mms)
{
    return mms->begin()->pd;
}

// map the page to the end of the mm_area in pd
int k_map(
    mm* mm_area,
    const struct page* page,
    int read,
    int write,
    int priv,
    int cow);

// @param len is aligned to 4kb boundary automatically, exceeding part will
// be filled with '0's and not written back to the file
int mmap(
    void* hint,
    size_t len,
    struct inode* file,
    size_t offset,
    int write,
    int priv);

// allocate a raw page
page_t alloc_raw_page(void);

// allocate n raw page(s)
// @return the id of the first page allocated
page_t alloc_n_raw_pages(size_t n);

// allocate a struct page together with the raw page
struct page allocate_page(void);

page_directory_entry* alloc_pd(void);
page_table_entry* alloc_pt(void);
