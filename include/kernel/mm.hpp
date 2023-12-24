#pragma once

#include <set>
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

#define memory_fence asm volatile("" ::: "memory")

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
    mutable uint32_t attr;
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

namespace kernel {

void* pmap(page_t pg, bool cached = true);
void pfree(page_t pg);

class paccess : public types::non_copyable {
private:
    page_t m_pg;
    void* m_ptr;

public:
    paccess(void) = delete;
    paccess(paccess&&) = delete;
    paccess& operator=(paccess&&) = delete;

    constexpr explicit paccess(page_t pg, bool cached = true)
        : m_pg(pg)
    {
        m_ptr = pmap(pg, cached);
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

namespace memory {

struct mm {
public:
    using pages_vector = std::vector<page, types::memory::ident_allocator<page>>;

public:
    void* start {};
    struct mm_attr {
        uint32_t write : 1;
        uint32_t system : 1;
        uint32_t mapped : 1;
    } attr {};
    pages_vector* pgs {};
    fs::inode* mapped_file {};
    size_t file_offset {};

public:
    constexpr void* end() const noexcept
    { return vptradd(start, pgs->size() * PAGE_SIZE); }
    constexpr bool is_kernel_space() const noexcept
    { return attr.system; }
    constexpr bool is_avail(void* ostart, void* oend) const noexcept
    {
        void* m_start = start;
        void* m_end = end();

        return (ostart >= m_end || oend <= m_start);
    }

    void append_page(pd_t pd, const page& pg, uint32_t attr, bool priv);

    /**
     * @brief Splits the memory block at the specified address.
     * 
     * @param addr The address at which the memory block will be split.
     * @return The new memory block created after splitting.
     */
    mm split(void* addr);

    constexpr bool operator<(const mm& rhs) const noexcept
    { return end() <= rhs.start; }
    constexpr bool operator<(void* rhs) const noexcept
    { return end() <= rhs; }
    friend constexpr bool operator<(void* lhs, const mm& rhs) noexcept
    { return lhs < rhs.start; }
};

class mm_list {
private:
    struct comparator {
        constexpr bool operator()(const mm& lhs, const mm& rhs) const noexcept
        { return lhs < rhs; }
        constexpr bool operator()(const mm& lhs, void* rhs) const noexcept
        { return lhs < rhs; }
        constexpr bool operator()(void* lhs, const mm& rhs) const noexcept
        { return lhs < rhs; }
    };

public:
    using list_type = std::set<mm, comparator, types::memory::ident_allocator<mm>>;
    using iterator = list_type::iterator;
    using const_iterator = list_type::const_iterator;

public:
    static inline mm_list* s_kernel_mms;

private:
    list_type m_areas;
    page_t m_pd;
    mm* m_brk {};

public:
    // for system initialization only
    explicit constexpr mm_list(page_t pd)
        : m_pd(pd) { }

    // default constructor copies kernel_mms
    explicit mm_list();
    // copies kernel_mms and mirrors user space
    explicit mm_list(const mm_list& other);

    constexpr mm_list(mm_list&& v)
        : m_areas(std::move(v.m_areas))
        , m_pd(std::exchange(v.m_pd, 0)) { }

    ~mm_list();
    void switch_pd() const;

    int register_brk(void* addr);
    void* set_brk(void* addr);

    void* find_avail(void* hint, size_t len, bool priv) const;

    int unmap(void* start, size_t len, bool priv);

    constexpr mm& addarea(void* start, bool w, bool system)
    {
        auto [ iter, inserted ] = m_areas.emplace(mm {
            .start = start,
            .attr {
                .write = w,
                .system = system,
                .mapped = 0,
            },
            .pgs = types::memory::kinew<mm::pages_vector>(),
        });
        assert(inserted);
        return *iter;
    }

    mm& add_empty_area(void* start, std::size_t page_count,
        uint32_t page_attr, bool w, bool system);

    constexpr void clear_user()
    {
        for (auto iter = m_areas.begin(); iter != m_areas.end(); ) {
            if (iter->is_kernel_space()) {
                ++iter;
                continue;
            }

            this->unmap(*iter);
            iter = m_areas.erase(iter);
        }
        m_brk = nullptr;
    }

    inline void unmap(mm& area)
    {
        int i = 0;

        // TODO:
        // if there are more than 4 pages, calling invlpg
        // should be faster. otherwise, we use movl cr3
        // bool should_invlpg = (area->pgs->size() > 4);

        for (auto& pg : *area.pgs) {
            kernel::paccess pa(pg.pg_pteidx >> 12);
            auto pt = (pt_t)pa.ptr();
            assert(pt);
            auto* pte = *pt + (pg.pg_pteidx & 0xfff);
            pte->v = 0;

            free_page(&pg);

            invalidate_tlb((uint32_t)area.start + (i++) * PAGE_SIZE);
        }
        types::memory::kidelete<mm::pages_vector>(area.pgs);
    }

    constexpr mm* find(void* lp)
    {
        auto iter = m_areas.find(lp);
        if (iter == m_areas.end())
            return nullptr;
        return &*iter;
    }
    constexpr const mm* find(void* lp) const
    {
        auto iter = m_areas.find(lp);
        if (iter == m_areas.end())
            return nullptr;
        return &*iter;
    }

    constexpr bool is_avail(void* start, size_t len) const noexcept
    {
        start = align_down<12>(start);
        len = vptrdiff(align_up<12>(vptradd(start, len)), start);
        for (const auto& area : m_areas) {
            if (!area.is_avail(start, vptradd(start, len)))
                return false;
        }
        return true;
    }

    constexpr bool is_avail(void* addr) const
    {
        auto iter = m_areas.find(addr);
        return iter == m_areas.end();
    }
};

} // namespace memory

} // namespace kernel

// global variables
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
