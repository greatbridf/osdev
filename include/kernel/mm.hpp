#pragma once

#include "types/size.h"
#include <kernel/mem.h>
#include <kernel/vfs.hpp>
#include <types/allocator.hpp>
#include <types/list.hpp>
#include <types/types.h>
#include <types/vector.hpp>

constexpr size_t THREAD_KERNEL_STACK_SIZE = 2 * PAGE_SIZE;

struct page {
    page_t phys_page_id;
    pte_t* pte;
    size_t* ref_count;
    union {
        uint32_t v;
        struct {
            uint32_t cow : 1;
        } in;
    } attr;
};

using page_arr = types::vector<page, types::kernel_ident_allocator>;

class mm {
public:
    void* start;
    union {
        uint32_t v;
        struct {
            uint32_t read : 1;
            uint32_t write : 1;
            uint32_t system : 1;
        } in;
    } attr;
    pd_t pd;
    page_arr* pgs;
    fs::inode* mapped_file;
    size_t file_offset;

public:
    mm(const mm& val);
    mm(void* start, pd_t pd, bool write, bool system);
};

using mm_list = types::list<mm, types::kernel_ident_allocator>;

// in mem.cpp
extern mm_list* kernel_mms;
extern page empty_page;

// translate physical address to virtual(mapped) address
void* p_ptr_to_v_ptr(phys_ptr_t p_ptr);

// translate linear address to physical address
phys_ptr_t l_ptr_to_p_ptr(const mm_list* mms, void* v_ptr);

// translate virtual(mapped) address to physical address
phys_ptr_t v_ptr_to_p_ptr(void* v_ptr);

// @return the pointer to the mm_area containing l_ptr
//         nullptr if not
mm* find_mm_area(mm_list* mms, void* l_ptr);

// find the corresponding page the l_ptr pointing to
// @return the pointer to the struct if found, NULL if not found
struct page* find_page_by_l_ptr(const mm_list* mms, void* l_ptr);

inline size_t vptrdiff(void* p1, void* p2)
{
    return (uint8_t*)p1 - (uint8_t*)p2;
}

inline page_t phys_addr_to_page(phys_ptr_t ptr)
{
    return ptr >> 12;
}

inline pd_i_t page_to_pd_i(page_t p)
{
    return p >> 10;
}

inline constexpr pt_i_t page_to_pt_i(page_t p)
{
    return p & (1024 - 1);
}

inline phys_ptr_t page_to_phys_addr(page_t p)
{
    return p << 12;
}

inline pd_i_t linr_addr_to_pd_i(void* ptr)
{
    return page_to_pd_i(phys_addr_to_page((phys_ptr_t)ptr));
}

inline pd_i_t linr_addr_to_pt_i(void* ptr)
{
    return page_to_pt_i(phys_addr_to_page((phys_ptr_t)ptr));
}

inline pd_t mms_get_pd(const mm_list* mms)
{
    return mms->begin()->pd;
}

inline void* to_vp(page_t pg)
{
    return p_ptr_to_v_ptr(page_to_phys_addr(pg));
}

inline pd_t to_pd(page_t pg)
{
    return reinterpret_cast<pd_t>(to_vp(pg));
}

inline pt_t to_pt(page_t pg)
{
    return reinterpret_cast<pt_t>(to_vp(pg));
}

inline pt_t to_pt(pde_t* pde)
{
    return to_pt(pde->in.pt_page);
}

inline pde_t* to_pde(pd_t pd, void* addr)
{
    return *pd + linr_addr_to_pd_i(addr);
}

inline pte_t* to_pte(pt_t pt, void* addr)
{
    return *pt + linr_addr_to_pt_i(addr);
}

inline pte_t* to_pte(pt_t pt, page_t pg)
{
    return *pt + page_to_pt_i(pg);
}

inline pte_t* to_pte(pde_t* pde, page_t pg)
{
    return to_pte(to_pt(pde), pg);
}

inline pte_t* to_pte(pde_t* pde, void* addr)
{
    return to_pte(to_pt(pde), addr);
}

inline pte_t* to_pte(pd_t pd, void* addr)
{
    return to_pte(to_pde(pd, addr), addr);
}

inline void* mmend(const mm* mm_area)
{
    return (char*)mm_area->start + mm_area->pgs->size() * PAGE_SIZE;
}

// map the page to the end of the mm_area in pd
int k_map(
    mm* mm_area,
    page* page,
    int read,
    int write,
    int priv,
    int cow);

// @param len is aligned to 4kb boundary automatically, exceeding part will
// be filled with '0's and not written back to the file
int mmap(
    void* hint,
    size_t len,
    fs::inode* file,
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

pd_t alloc_pd(void);
pt_t alloc_pt(void);
