#pragma once

#include <set>

#include <stdint.h>

#include "vm_area.hpp"
#include "paging.hpp"

namespace kernel::mem {

constexpr uintptr_t KERNEL_SPACE_START    = 0x8000000000000000ULL;
constexpr uintptr_t USER_SPACE_MEMORY_TOP = 0x0000800000000000ULL;
constexpr uintptr_t MMAP_MIN_ADDR         = 0x0000000000001000ULL;
constexpr uintptr_t STACK_MIN_ADDR        = 0x0000700000000000ULL;

class mm_list {
private:
    struct comparator {
        constexpr bool operator()(const vm_area& lhs, const vm_area& rhs) const noexcept
        { return lhs < rhs; }
        constexpr bool operator()(const vm_area& lhs, uintptr_t rhs) const noexcept
        { return lhs < rhs; }
        constexpr bool operator()(uintptr_t lhs, const vm_area& rhs) const noexcept
        { return lhs < rhs; }
    };

public:
    using list_type = std::set<vm_area, comparator>;
    using iterator = list_type::iterator;
    using const_iterator = list_type::const_iterator;

    struct map_args {
        // MUSE BE aligned to 4kb boundary
        uintptr_t vaddr;
        // MUSE BE aligned to 4kb boundary
        std::size_t length;

        unsigned long flags;

        fs::inode* file_inode;
        // MUSE BE aligned to 4kb boundary
        std::size_t file_offset;
    };

private:
    list_type m_areas;
    paging::pfn_t m_pt;
    iterator m_brk {};

public:
    // default constructor copies kernel_mms
    explicit mm_list();
    // copies kernel_mms and mirrors user space
    explicit mm_list(const mm_list& other);

    constexpr mm_list(mm_list&& v)
        : m_areas(std::move(v.m_areas))
        , m_pt(std::exchange(v.m_pt, 0))
        , m_brk{std::move(v.m_brk)} { }

    ~mm_list();

    void switch_pd() const noexcept;

    int register_brk(uintptr_t addr);
    uintptr_t set_brk(uintptr_t addr);

    void clear();

    // split the memory block at the specified address
    // return: iterator to the new block
    iterator split(iterator area, uintptr_t at);

    bool is_avail(uintptr_t addr) const;
    bool is_avail(uintptr_t start, std::size_t length) const noexcept;

    uintptr_t find_avail(uintptr_t hint, size_t length) const;

    int unmap(iterator area);
    int unmap(uintptr_t start, std::size_t length);

    int mmap(const map_args& args);

    constexpr vm_area* find(uintptr_t lp)
    {
        auto iter = m_areas.find(lp);
        if (iter == m_areas.end())
            return nullptr;
        return &iter;
    }

    constexpr const vm_area* find(uintptr_t lp) const
    {
        auto iter = m_areas.find(lp);
        if (iter == m_areas.end())
            return nullptr;
        return &iter;
    }

    constexpr paging::PSE get_page_table() const noexcept
    {
        return paging::PSE {m_pt};
    }
};

} // namespace kernel::mem
