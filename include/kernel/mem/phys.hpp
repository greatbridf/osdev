#pragma once

#include <bit>
#include <cstddef>

#include <stdint.h>

#include <types/types.h>

#include <kernel/mem/types.hpp>

namespace kernel::mem {

template <typename T, bool Cached = true>
class physaddr {
    static constexpr uintptr_t PHYS_OFFSET = Cached ? 0xffffff0000000000ULL : 0xffffff4000000000ULL;

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

} // namespace kernel::mem
