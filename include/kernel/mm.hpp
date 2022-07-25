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

// allocate n raw page(s)
// @return the id of the first page allocated
page_t alloc_n_raw_pages(size_t n);
void free_n_raw_pages(page_t start_pg, size_t n);

// forward declaration
namespace kernel {
class mm_list;
} // namespace kernel

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
    kernel::mm_list* owner;
    page_arr* pgs = nullptr;
    fs::inode* mapped_file = nullptr;
    size_t file_offset = 0;

public:
    constexpr void* end(void) const
    {
        return (char*)this->start + this->pgs->size() * PAGE_SIZE;
    }

    inline bool is_ident(void) const
    {
        return this->end() <= (void*)0x40000000U;
    }

    constexpr bool is_avail(void* start, void* end) const
    {
        void* m_start = this->start;
        void* m_end = this->end();

        return (start >= m_end || end <= m_start);
    }

    int append_page(page* pg, bool present, bool write, bool priv, bool cow);
};

namespace kernel {

class mm_list {
public:
    using list_type = ::types::list<mm, types::kernel_ident_allocator>;
    using iterator_type = list_type::iterator_type;
    using const_iterator_type = list_type::const_iterator_type;

private:
    list_type m_areas;

public:
    pd_t m_pd;

public:
    explicit mm_list(pd_t pd);
    mm_list(const mm_list& v);
    constexpr mm_list(mm_list&& v)
        : m_areas(::types::move(v.m_areas))
        , m_pd(v.m_pd)
    {
        v.m_pd = nullptr;
    }
    ~mm_list();

    constexpr iterator_type begin(void)
    {
        return m_areas.begin();
    }
    constexpr iterator_type end(void)
    {
        return m_areas.end();
    }
    constexpr const_iterator_type begin(void) const
    {
        return m_areas.begin();
    }
    constexpr const_iterator_type end(void) const
    {
        return m_areas.end();
    }
    constexpr const_iterator_type cbegin(void) const
    {
        return m_areas.cbegin();
    }
    constexpr const_iterator_type cend(void) const
    {
        return m_areas.cend();
    }

    constexpr iterator_type addarea(void* start, bool w, bool system)
    {
        return m_areas.emplace_back(mm {
            .start = start,
            .attr {
                .in {
                    .read = 1,
                    .write = w,
                    .system = system,
                },
            },
            .owner = this,
            .pgs = ::types::kernel_ident_allocator_new<page_arr>(),
        });
    }

    constexpr void clear_user()
    {
        for (auto iter = this->begin(); iter != this->end();) {
            if (iter->is_ident())
                ++iter;

            // TODO:
            // k_unmap(iter.ptr());
            iter = m_areas.erase(iter);
        }
    }

    constexpr int mirror_area(mm& src)
    {
        auto area = this->addarea(
            src.start, src.attr.in.write, src.attr.in.system);
        if (src.mapped_file) {
            area->mapped_file = src.mapped_file;
            area->file_offset = src.file_offset;
        }

        for (auto& pg : *src.pgs) {
            if (area->append_page(&pg,
                    true,
                    src.attr.in.write,
                    src.attr.in.system,
                    true)
                != GB_OK) {
                return GB_FAILED;
            }
        }

        return GB_OK;
    }

    constexpr void unmap(iterator_type area)
    {
        for (auto& pg : *area->pgs) {
            if (*pg.ref_count == 1) {
                ki_free(pg.ref_count);
                free_n_raw_pages(pg.phys_page_id, 1);
            } else {
                --*pg.ref_count;
            }

            pg.phys_page_id = 0;
            pg.attr.v = 0;
            pg.pte->v = 0;
        }
        area->attr.v = 0;
        area->start = 0;
    }

    constexpr iterator_type find(void* lp)
    {
        for (auto iter = this->begin(); iter != this->end(); ++iter)
            if (lp >= iter->start && lp < iter->end())
                return iter;

        return this->end();
    }
};

} // namespace kernel

// global variables
inline kernel::mm_list* kernel_mms;
inline page empty_page;
// --------------------------------

// translate physical address to virtual(mapped) address
void* ptovp(pptr_t p_ptr);

inline constexpr size_t vptrdiff(void* p1, void* p2)
{
    return (uint8_t*)p1 - (uint8_t*)p2;
}
inline constexpr page* lto_page(mm* mm_area, void* l_ptr)
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

// allocate a raw page
inline page_t alloc_raw_page(void)
{
    return alloc_n_raw_pages(1);
}

// allocate a struct page together with the raw page
struct page allocate_page(void);

pd_t alloc_pd(void);
pt_t alloc_pt(void);

void dealloc_pd(pd_t pd);
void dealloc_pt(pt_t pt);
