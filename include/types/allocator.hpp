#pragma once
#include <cstddef>

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
    byte* p_allocated;
    kernel::async::mutex mtx;

    byte* brk(byte* addr);
    byte* sbrk(size_type increment);

    constexpr byte* sbrk() const noexcept { return p_break; }

   public:
    explicit brk_memory_allocator(byte* start, size_type size);
    brk_memory_allocator(const brk_memory_allocator&) = delete;

    void* allocate(size_type size);
    void deallocate(void* ptr);

    bool allocated(void* ptr) const noexcept;
};

} // namespace types::memory
