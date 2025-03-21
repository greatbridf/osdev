#pragma once

#include <cstddef>
#include <tuple>

#include <stdint.h>

#include <kernel/mem/paging_asm.h>
#include <kernel/mem/phys.hpp>

namespace kernel::mem::paging {

constexpr int idx_p5(uintptr_t vaddr) noexcept {
    return (vaddr >> 48) & 0x1ff;
}
constexpr int idx_p4(uintptr_t vaddr) noexcept {
    return (vaddr >> 39) & 0x1ff;
}
constexpr int idx_p3(uintptr_t vaddr) noexcept {
    return (vaddr >> 30) & 0x1ff;
}
constexpr int idx_p2(uintptr_t vaddr) noexcept {
    return (vaddr >> 21) & 0x1ff;
}
constexpr int idx_p1(uintptr_t vaddr) noexcept {
    return (vaddr >> 12) & 0x1ff;
}

constexpr std::tuple<int, int, int, int, int> idx_all(uintptr_t vaddr) noexcept {
    return {idx_p5(vaddr), idx_p4(vaddr), idx_p3(vaddr), idx_p2(vaddr), idx_p1(vaddr)};
}

// page frame number
// since we have large pages now, pfns are not shifted right
using pfn_t = uintptr_t;

// paging structure attributes
using psattr_t = uintptr_t;

constexpr psattr_t PA_DATA = PA_P | PA_RW | PA_NXE;
constexpr psattr_t PA_KERNEL_DATA = PA_DATA | PA_G;
constexpr psattr_t PA_USER_DATA = PA_DATA | PA_G | PA_US;

constexpr psattr_t PA_PAGE_TABLE = PA_P | PA_RW;
constexpr psattr_t PA_KERNEL_PAGE_TABLE = PA_PAGE_TABLE | PA_G;

constexpr psattr_t PA_DATA_HUGE = PA_DATA | PA_PS;
constexpr psattr_t PA_KERNEL_DATA_HUGE = PA_DATA_HUGE | PA_G;
constexpr psattr_t PA_USER_DATA_HUGE = PA_DATA_HUGE | PA_US;

namespace __inner {
    using pse_t = uint64_t;

} // namespace __inner

class PSE {
    physaddr<__inner::pse_t> m_ptrbase;

   public:
    explicit constexpr PSE(uintptr_t pptr) noexcept : m_ptrbase{pptr} {}

    constexpr void clear() noexcept { *m_ptrbase = 0; }

    constexpr void set(psattr_t attributes, pfn_t pfn) {
        *m_ptrbase = (attributes & PA_MASK) | (pfn & ~PA_MASK);
    }

    constexpr pfn_t pfn() const noexcept { return *m_ptrbase & ~PA_MASK; }

    constexpr psattr_t attributes() const noexcept { return *m_ptrbase & PA_MASK; }

    constexpr PSE operator[](std::size_t nth) const noexcept {
        return PSE{m_ptrbase.phys() + 8 * nth};
    }

    constexpr PSE parse() const noexcept { return PSE{*m_ptrbase & ~PA_MASK}; }
};

constexpr physaddr<void> KERNEL_PAGE_TABLE_PHYS_ADDR{KERNEL_PML4};
constexpr PSE KERNEL_PAGE_TABLE{KERNEL_PML4};

constexpr unsigned long PAGE_PRESENT = 0x00010000;
constexpr unsigned long PAGE_BUDDY = 0x00020000;
constexpr unsigned long PAGE_SLAB = 0x00040000;

struct page {
    // TODO: use atomic
    unsigned long refcount;
    unsigned long flags;

    page* next;
    page* prev;
};

inline page* PAGE_ARRAY;

constexpr unsigned long PAGE_FAULT_P = 0x00000001;
constexpr unsigned long PAGE_FAULT_W = 0x00000002;
constexpr unsigned long PAGE_FAULT_U = 0x00000004;
constexpr unsigned long PAGE_FAULT_R = 0x00000008;
constexpr unsigned long PAGE_FAULT_I = 0x00000010;
constexpr unsigned long PAGE_FAULT_PK = 0x00000020;
constexpr unsigned long PAGE_FAULT_SS = 0x00000040;
constexpr unsigned long PAGE_FAULT_SGX = 0x00008000;

} // namespace kernel::mem::paging

struct Page;

extern "C" Page* c_alloc_page();
extern "C" Page* c_alloc_pages(uint32_t order);
extern "C" uintptr_t c_alloc_page_table();
extern "C" uintptr_t page_to_pfn(Page* page);
