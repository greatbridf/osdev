#pragma once
#include <memory>
#include <new>
#include <utility>
#include <type_traits>
#include <cstddef>

#include <stdint.h>
#include <types/cplusplus.hpp>
#include <types/types.h>

#include <kernel/async/lock.hpp>

namespace types::memory {

class brk_memory_allocator {
public:
    using byte = std::byte;
    using size_type = std::size_t;

private:
    byte* p_start;
    byte* p_limit;
    byte* p_break;
    kernel::async::mutex mtx;

    constexpr byte* brk(byte* addr)
    {
        if (addr >= p_limit) [[unlikely]]
            return nullptr;
        return p_break = addr;
    }

    constexpr byte* sbrk(size_type increment)
    { return brk(p_break + increment); }

public:
    explicit brk_memory_allocator(byte* start, size_type size);
    brk_memory_allocator(const brk_memory_allocator&) = delete;

    void* allocate(size_type size);
    void deallocate(void* ptr);
};

} // namespace types::memory
