#pragma once

#include <set>
#include <vector>
#include <bit>
#include <cstddef>
#include <utility>

#include <kernel/mem/paging.hpp>
#include <kernel/vfs.hpp>
#include <stdint.h>
#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/types.h>

#define invalidate_tlb(addr) asm volatile("invlpg (%0)": : "r"(addr) : "memory")

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

template <int N>
constexpr std::size_t align_down(std::size_t v)
{
    return v & ~((1 << N) - 1);
}
template <int N>
constexpr void* align_down(void* v)
{
    return std::bit_cast<void*>(align_down<N>(std::bit_cast<std::size_t>(v)));
}
template <int N>
constexpr std::size_t align_up(std::size_t v)
{
    return align_down<N>(v + (1 << N) - 1);
}
template <int N>
constexpr void* align_up(void* v)
{
    return std::bit_cast<void*>(align_up<N>(std::bit_cast<std::size_t>(v)));
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

// TODO: LONG MODE
// void dealloc_pd(page_t pd);

// allocate a struct page together with the raw page
kernel::mem::paging::page allocate_page(void);
void free_page(kernel::mem::paging::page* pg);

namespace kernel {

namespace mem {

struct mm {
public:
    void* start {};
    struct mm_attr {
        uint32_t write : 1;
        uint32_t system : 1;
        uint32_t mapped : 1;
    } attr {};
    fs::inode* mapped_file {};
    size_t file_offset {};
    std::size_t page_count;

public:
    constexpr void* end() const noexcept
    { return vptradd(start, page_count * 4096); } // TODO: LONG MODE
    constexpr bool is_kernel_space() const noexcept
    { return attr.system; }
    constexpr bool is_avail(void* ostart, void* oend) const noexcept
    {
        void* m_start = start;
        void* m_end = end();

        return (ostart >= m_end || oend <= m_start);
    }

    // void append_page(pd_t pd, const page& pg, uint32_t attr, bool priv); TODO: LONG MODE

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
    // TODO: LONG MODE: use slab allocator
    using list_type = std::set<mm, comparator>;
    using iterator = list_type::iterator;
    using const_iterator = list_type::const_iterator;

public:
    static inline mm_list* s_kernel_mms;

private:
    list_type m_areas;
    kernel::mem::paging::pfn_t m_pd;
    mm* m_brk {};

public:
    // for system initialization only
    explicit constexpr mm_list(kernel::mem::paging::pfn_t pd)
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

            // TODO: LONG MODE
            // this->unmap(*iter);
            iter = m_areas.erase(iter);
        }
        m_brk = nullptr;
    }

    // TODO: LONG MODE
    // inline void unmap(mm& area)
    // {
    //     int i = 0;

    //     // TODO:
    //     // if there are more than 4 pages, calling invlpg
    //     // should be faster. otherwise, we use movl cr3
    //     // bool should_invlpg = (area->pgs->size() > 4);

    //     for (auto& pg : *area.pgs) {
    //         kernel::paccess pa(pg.pg_pteidx >> 12);
    //         auto pt = (pt_t)pa.ptr();
    //         assert(pt);
    //         auto* pte = *pt + (pg.pg_pteidx & 0xfff);
    //         pte->v = 0;

    //         free_page(&pg);

    //         invalidate_tlb((std::size_t)area.start + (i++) * PAGE_SIZE);
    //     }
    //     types::memory::kidelete<mm::pages_vector>(area.pgs);
    // }

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
