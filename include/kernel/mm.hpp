#pragma once

#include <kernel/mem.h>
#include <kernel/vfs.hpp>
#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/list.hpp>
#include <types/size.h>
#include <types/status.h>
#include <types/types.h>
#include <types/vector.hpp>

#define invalidate_tlb(addr) asm("invlpg (%0)" \
                                 :             \
                                 : "r"(addr)   \
                                 : "memory")

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

struct mm;

// map the page to the end of the mm_area in pd
int k_map(
    mm* mm_area,
    page* page,
    int read,
    int write,
    int priv,
    int cow);

// private memory mapping
// changes won't be neither written back to file nor shared between processes
// TODO: shared mapping
// @param len is aligned to 4kb boundary automatically, exceeding part will
// be filled with '0's and not written back to the file
int mmap(
    void* hint,
    size_t len,
    fs::inode* file,
    size_t offset,
    int write,
    int priv);

using page_arr = types::vector<page, types::kernel_ident_allocator>;

using mm_list = types::list<mm, types::kernel_ident_allocator>;

struct mm {
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
    page_arr* pgs = types::kernel_ident_allocator_new<page_arr>();
    fs::inode* mapped_file = nullptr;
    size_t file_offset = 0;

public:
    static constexpr int mirror_mm_area(mm_list* dst, const mm* src, pd_t pd)
    {
        mm new_nn {
            .start = src->start,
            .attr { src->attr.v },
            .pd = pd,
            .mapped_file = src->mapped_file,
            .file_offset = src->file_offset,
        };

        for (auto iter = src->pgs->begin(); iter != src->pgs->end(); ++iter) {
            if (k_map(&new_nn, &*iter,
                    src->attr.in.read, src->attr.in.write, src->attr.in.system, 1)
                != GB_OK) {
                return GB_FAILED;
            }
        }

        dst->emplace_back(types::move(new_nn));
        return GB_OK;
    }
};

// in mem.cpp
extern mm_list* kernel_mms;
extern page empty_page;

// translate physical address to virtual(mapped) address
void* ptovp(pptr_t p_ptr);

// @return the pointer to the mm_area containing l_ptr
//         nullptr if not
mm* find_mm_area(mm_list* mms, void* l_ptr);

inline constexpr size_t vptrdiff(void* p1, void* p2)
{
    return (uint8_t*)p1 - (uint8_t*)p2;
}
inline constexpr page* lto_page(const mm* mm_area, void* l_ptr)
{
    size_t offset = vptrdiff(l_ptr, mm_area->start);
    return &mm_area->pgs->at(offset / PAGE_SIZE);
}
inline constexpr page_t to_page(pptr_t ptr)
{
    return ptr >> 12;
}
inline constexpr size_t to_pdi(page_t pg)
{
    return pg >> 10;
}
inline constexpr size_t to_pti(page_t pg)
{
    return pg & (1024 - 1);
}
inline constexpr pptr_t to_pp(page_t p)
{
    return p << 12;
}
inline constexpr size_t lto_pdi(pptr_t ptr)
{
    return to_pdi(to_page(ptr));
}
inline constexpr size_t lto_pti(pptr_t ptr)
{
    return to_pti(to_page(ptr));
}
inline constexpr pte_t* to_pte(pt_t pt, page_t pg)
{
    return *pt + to_pti(pg);
}
inline pd_t mms_get_pd(const mm_list* mms)
{
    return mms->begin()->pd;
}
inline void* to_vp(page_t pg)
{
    return ptovp(to_pp(pg));
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
    return *pd + lto_pdi((pptr_t)addr);
}
inline pte_t* to_pte(pt_t pt, void* addr)
{
    return *pt + lto_pti((pptr_t)addr);
}
inline pte_t* to_pte(pde_t* pde, void* addr)
{
    return to_pte(to_pt(pde), addr);
}
inline pte_t* to_pte(pd_t pd, void* addr)
{
    return to_pte(to_pde(pd, addr), addr);
}
inline pte_t* to_pte(pde_t* pde, page_t pg)
{
    return to_pte(to_pt(pde), pg);
}
inline constexpr void* mmend(const mm* mm_area)
{
    return (char*)mm_area->start + mm_area->pgs->size() * PAGE_SIZE;
}

// allocate a raw page
page_t alloc_raw_page(void);

// allocate n raw page(s)
// @return the id of the first page allocated
page_t alloc_n_raw_pages(size_t n);

// allocate a struct page together with the raw page
struct page allocate_page(void);

pd_t alloc_pd(void);
pt_t alloc_pt(void);
