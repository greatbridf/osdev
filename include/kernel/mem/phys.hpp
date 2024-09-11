#pragma once

#include <bit>
#include <cstddef>

#include <stdint.h>

#include <types/types.h>

#include <kernel/mem/types.hpp>

namespace kernel::mem {

template <typename T, bool Cached = true>
class physaddr {
    static constexpr uintptr_t PHYS_OFFSET =
        Cached ? 0xffffff0000000000ULL : 0xffffff4000000000ULL;

    uintptr_t m_ptr;

   public:
    explicit constexpr physaddr(uintptr_t ptr) : m_ptr{ptr} {}
    explicit constexpr physaddr(std::nullptr_t) : m_ptr{} {}

    // cast to non-pointer types is prohibited
    template <typename U, typename = std::enable_if_t<std::is_pointer_v<U>>>
    constexpr U cast_to() const noexcept {
        return std::bit_cast<U>(m_ptr + PHYS_OFFSET);
    }

    constexpr operator T*() const noexcept { return cast_to<T*>(); }

    constexpr T* operator->() const noexcept { return *this; }

    constexpr uintptr_t phys() const noexcept { return m_ptr; }
};

//  gdt[0]:  null
//  gdt[1]:  kernel code
//  gdt[2]:  kernel data
//  gdt[3]:  user code
//  gdt[4]:  user data
//  gdt[5]:  user code compability mode
//  gdt[6]:  user data compability mode
//  gdt[7]:  thread local 32bit
//  gdt[8]:  tss descriptor low
//  gdt[9]:  tss descriptor high
//  gdt[10]: ldt descriptor low
//  gdt[11]: ldt descriptor high
//  gdt[12]: null segment(in ldt)
//  gdt[13]: thread local 64bit(in ldt)
// &gdt[14]: tss of 0x68 bytes from here
constexpr physaddr<uint64_t> gdt{0x00000000 + 1 - 1};

} // namespace kernel::mem
