#pragma once

#include <list>
#include <vector>
#include <bit>
#include <cstddef>
#include <utility>

#include <kernel/mem.h>
#include <kernel/vfs.hpp>
#include <stdint.h>
#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/size.h>
#include <types/status.h>
#include <types/types.h>

#define invalidate_tlb(addr) asm("invlpg (%0)" \
                                 :             \
                                 : "r"(addr)   \
                                 : "memory")

constexpr size_t THREAD_KERNEL_STACK_SIZE = 2 * PAGE_SIZE;

constexpr uint32_t PAGE_COW = (1 << 0);
constexpr uint32_t PAGE_MMAP = (1 << 1);
#define PAGE_COW PAGE_COW
#define PAGE_MMAP PAGE_MMAP

struct page {
    page_t phys_page_id;
    size_t* ref_count;
    // 0 :11 : pte_index
    // 12:31 : pt_page
    uint32_t pg_pteidx;
    uint32_t attr;
};

// private memory mapping
// changes won't be neither written back to file nor shared between processes
// TODO: shared mapping
// @param len is aligned to 4kb boundary automatically, exceeding part will
// be filled with '0's and not written back to the file
// @param offset MUST be aligned to 4kb
int mmap(
    void* hint,
    size_t len,
    fs::inode* file,
    size_t offset,
    int write,
    int priv);

using page_arr = std::vector<page,
    types::allocator_adapter<page, types::kernel_ident_allocator>>;

// forward declaration
namespace kernel {
class mm_list;
} // namespace kernel

template <uint32_t base, uint32_t expo>
constexpr uint32_t pow()
{
    if constexpr (expo == 0)
        return 1;
    if constexpr (expo == 1)
        return base;
    if constexpr (expo % 2 == 0)
        return pow<base, expo / 2>() * pow<base, expo / 2>();
    else
        return pow<base, expo / 2>() * pow<base, expo / 2 + 1>();
}

template <int N>
constexpr uint32_t align_down(uint32_t v)
{
    return v & ~(pow<2, N>() - 1);
}
template <int N>
constexpr void* align_down(void* v)
{
    return std::bit_cast<void*>(align_down<N>(std::bit_cast<uint32_t>(v)));
}
template <int N>
constexpr uint32_t align_up(uint32_t v)
{
    return align_down<N>(v + pow<2, N>() - 1);
}
template <int N>
constexpr void* align_up(void* v)
{
    return std::bit_cast<void*>(align_up<N>(std::bit_cast<uint32_t>(v)));
}

constexpr size_t vptrdiff(void* p1, void* p2)
{
    auto* _p1 = static_cast<std::byte*>(p1);
    auto* _p2 = static_cast<std::byte*>(p2);
    return _p1 - _p2;
}

constexpr void* vptradd(void* p, std::size_t off)
{
    auto* _p = static_cast<std::byte*>(p);
    return _p + off;
}

void dealloc_pd(page_t pd);

// allocate a struct page together with the raw page
page allocate_page(void);
void free_page(page* pg);

// TODO: this is for alloc_kstack()
// CHANGE THIS
page_t __alloc_raw_page(void);
void __free_raw_page(page_t pg);

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

    constexpr bool is_kernel_space(void) const
    {
        return this->start >= std::bit_cast<void*>(0xc0000000);
    }

    constexpr bool is_avail(void* start, void* end) const
    {
        void* m_start = this->start;
        void* m_end = this->end();

        return (start >= m_end || end <= m_start);
    }

    int append_page(page& pg, uint32_t attr, bool priv);
};

namespace kernel {

void* pmap(page_t pg);
void pfree(page_t pg);

class paccess : public types::non_copyable {
private:
    page_t m_pg;
    void* m_ptr;

public:
    paccess(void) = delete;
    paccess(paccess&&) = delete;
    paccess& operator=(paccess&&) = delete;

    constexpr explicit paccess(page_t pg)
        : m_pg(pg)
    {
        m_ptr = pmap(pg);
    }
    constexpr void* ptr(void) const
    {
        return m_ptr;
    }
    ~paccess()
    {
        pfree(m_pg);
    }
};

class mm_list {
public:
    using list_type = std::list<mm,
        types::allocator_adapter<mm, types::kernel_ident_allocator>>;
    using iterator_type = list_type::iterator;
    using const_iterator_type = list_type::const_iterator;

private:
    list_type m_areas;

public:
    page_t m_pd;

public:
    explicit constexpr mm_list(page_t pd)
        : m_pd(pd)
    {
    }
    mm_list(const mm_list& v);
    constexpr mm_list(mm_list&& v)
        : m_areas(std::move(v.m_areas))
        , m_pd(v.m_pd)
    {
        v.m_pd = 0;
        for (auto& area : m_areas)
            area.owner = this;
    }
    ~mm_list()
    {
        if (!m_pd)
            return;

        this->clear_user();
        dealloc_pd(m_pd);
    }

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

    constexpr mm& addarea(void* start, bool w, bool system)
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
            .pgs = types::_new<types::kernel_ident_allocator, page_arr>(),
        });
    }

    constexpr void clear_user()
    {
        for (auto iter = this->begin(); iter != this->end();) {
            if (iter->is_kernel_space()) {
                ++iter;
                continue;
            }

            this->unmap(iter);

            iter = m_areas.erase(iter);
        }
    }

    constexpr int mirror_area(mm& src)
    {
        auto& area = this->addarea(
            src.start, src.attr.in.write, src.attr.in.system);

        if (src.mapped_file) {
            area.mapped_file = src.mapped_file;
            area.file_offset = src.file_offset;
        }

        for (auto& pg : *src.pgs) {
            if (area.append_page(pg,
                    PAGE_COW | (pg.attr & PAGE_MMAP),
                    src.attr.in.system)
                != GB_OK) {
                return GB_FAILED;
            }
        }

        return GB_OK;
    }

    inline void unmap(iterator_type area)
    {
        int i = 0;

        // TODO:
        // if there are more than 4 pages, calling invlpg
        // should be faster. otherwise, we use movl cr3
        // bool should_invlpg = (area->pgs->size() > 4);

        for (auto& pg : *area->pgs) {
            kernel::paccess pa(pg.pg_pteidx >> 12);
            auto pt = (pt_t)pa.ptr();
            assert(pt);
            auto* pte = *pt + (pg.pg_pteidx & 0xfff);
            pte->v = 0;

            free_page(&pg);

            invalidate_tlb((uint32_t)area->start + (i++) * PAGE_SIZE);
        }
        types::pdelete<types::kernel_ident_allocator>(area->pgs);
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

    constexpr bool is_avail(void* start, size_t len)
    {
        start = align_down<12>(start);
        len = vptrdiff(align_up<12>(vptradd(start, len)), start);
        for (const auto& area : *this) {
            if (!area.is_avail(start, (char*)start + len))
                return false;
        }

        return true;
    }
};

} // namespace kernel

// global variables
inline kernel::mm_list* kernel_mms;
inline page empty_page;
// --------------------------------

// inline constexpr page* lto_page(mm* mm_area, void* l_ptr)
// {
//     size_t offset = vptrdiff(l_ptr, mm_area->start);
//     return &mm_area->pgs->at(offset / PAGE_SIZE);
// }
// inline constexpr page_t to_page(pptr_t ptr)
// {
//     return ptr >> 12;
// }
// inline constexpr size_t to_pdi(page_t pg)
// {
//     return pg >> 10;
// }
// inline constexpr size_t to_pti(page_t pg)
// {
//     return pg & (1024 - 1);
// }
// inline constexpr pptr_t to_pp(page_t p)
// {
//     return p << 12;
// }
constexpr size_t v_to_pdi(void* addr)
{
    return std::bit_cast<uint32_t>(addr) >> 22;
}
constexpr size_t v_to_pti(void* addr)
{
    return (std::bit_cast<uint32_t>(addr) >> 12) & 0x3ff;
}
// inline constexpr pte_t* to_pte(pt_t pt, page_t pg)
// {
//     return *pt + to_pti(pg);
// }
// inline void* to_vp(page_t pg)
// {
//     return ptovp(to_pp(pg));
// }
// inline pd_t to_pd(page_t pg)
// {
//     return reinterpret_cast<pd_t>(to_vp(pg));
// }
// inline pt_t to_pt(page_t pg)
// {
//     return reinterpret_cast<pt_t>(to_vp(pg));
// }
// inline pt_t to_pt(pde_t* pde)
// {
//     return to_pt(pde->in.pt_page);
// }
// inline pde_t* to_pde(pd_t pd, void* addr)
// {
//     return *pd + lto_pdi((pptr_t)addr);
// }
// inline pte_t* to_pte(pt_t pt, void* addr)
// {
//     return *pt + lto_pti((pptr_t)addr);
// }
// inline pte_t* to_pte(pde_t* pde, void* addr)
// {
//     return to_pte(to_pt(pde), addr);
// }
// inline pte_t* to_pte(pd_t pd, void* addr)
// {
//     return to_pte(to_pde(pd, addr), addr);
// }
// inline pte_t* to_pte(pde_t* pde, page_t pg)
// {
//     return to_pte(to_pt(pde), pg);
// }
