#pragma once

#include <bit>
#include <tuple>
#include <cstddef>

#include <stdint.h>

#include <types/types.h>

#include <kernel/mem/phys.hpp>

namespace kernel::mem::paging {

constexpr int idx_p5(uintptr_t vaddr) noexcept { return (vaddr >> 48) & 0x1ff; }
constexpr int idx_p4(uintptr_t vaddr) noexcept { return (vaddr >> 39) & 0x1ff; }
constexpr int idx_p3(uintptr_t vaddr) noexcept { return (vaddr >> 30) & 0x1ff; }
constexpr int idx_p2(uintptr_t vaddr) noexcept { return (vaddr >> 21) & 0x1ff; }
constexpr int idx_p1(uintptr_t vaddr) noexcept { return (vaddr >> 12) & 0x1ff; }

constexpr std::tuple<int, int, int, int, int> idx_all(uintptr_t vaddr) noexcept
{
    return {idx_p5(vaddr), idx_p4(vaddr), idx_p3(vaddr), idx_p2(vaddr), idx_p1(vaddr)};
}

// page frame number
// since we have large pages now, pfns are not shifted right
using pfn_t = uintptr_t;

// paging structure attributes
using psattr_t = uintptr_t;

constexpr psattr_t PA_P    = 0x0000000000000001ULL;
constexpr psattr_t PA_RW   = 0x0000000000000002ULL;
constexpr psattr_t PA_US   = 0x0000000000000004ULL;
constexpr psattr_t PA_PWT  = 0x0000000000000008ULL;
constexpr psattr_t PA_PCD  = 0x0000000000000010ULL;
constexpr psattr_t PA_A    = 0x0000000000000020ULL;
constexpr psattr_t PA_D    = 0x0000000000000040ULL;
constexpr psattr_t PA_PS   = 0x0000000000000080ULL;
constexpr psattr_t PA_G    = 0x0000000000000100ULL;
constexpr psattr_t PA_COW  = 0x0000000000000200ULL; // copy on write
constexpr psattr_t PA_MMAP = 0x0000000000000400ULL; // memory mapped
constexpr psattr_t PA_FRE  = 0x0000000000000800ULL; // unused flag
constexpr psattr_t PA_NXE  = 0x8000000000000000ULL;
constexpr psattr_t PA_MASK = 0xfff0000000000fffULL;

constexpr psattr_t PA_DATA = PA_P | PA_RW | PA_NXE;

constexpr psattr_t PA_PAGE_TABLE = PA_DATA;
constexpr psattr_t PA_KERNEL_PAGE_TABLE = PA_DATA;

constexpr psattr_t PA_KERNEL_DATA = PA_DATA | PA_G;
constexpr psattr_t PA_KERNEL_DATA_HUGE = PA_KERNEL_DATA | PA_PS;

namespace __inner {
    using pse_t = uint64_t;

} // namespace __inner

class PSE {
    physaddr<__inner::pse_t> m_ptrbase;

public:
    explicit constexpr PSE(uintptr_t pptr) noexcept : m_ptrbase{pptr} {}

    constexpr void clear() noexcept
    {
        *m_ptrbase = 0;
    }

    constexpr void set(psattr_t attributes, pfn_t pfn)
    {
        *m_ptrbase = (attributes & PA_MASK) | (pfn & ~PA_MASK);
    }

    constexpr pfn_t pfn() const noexcept
    {
        return *m_ptrbase & ~PA_MASK;
    }

    constexpr psattr_t attributes() const noexcept
    {
        return *m_ptrbase & PA_MASK;
    }

    constexpr PSE operator[](std::size_t nth) const noexcept
    {
        return PSE{m_ptrbase.phys() + 8 * nth};
    }

    constexpr PSE parse() const noexcept
    {
        return PSE{*m_ptrbase & ~PA_MASK};
    }
};

constexpr PSE KERNEL_PAGE_TABLE{0x100000};

constexpr unsigned long PAGE_PRESENT = 0x00000001;
constexpr unsigned long PAGE_BUDDY   = 0x00000002;
constexpr unsigned long PAGE_SLAB    = 0x00000004;

struct page {
    refcount_t refcount;
    unsigned long flags;

    page* next;

    // padding
    uint64_t padding;
};

inline page* PAGE_ARRAY;

void create_zone(uintptr_t start, uintptr_t end);
void mark_present(uintptr_t start, uintptr_t end);

// order represents power of 2
page* alloc_page();
page* alloc_pages(int order);
void free_page(page* page, int order);

pfn_t alloc_page_table();

pfn_t page_to_pfn(page* page);
page* pfn_to_page(pfn_t pfn);

} // namespace kernel::mem::paging
