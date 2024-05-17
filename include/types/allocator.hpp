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

namespace kernel::kinit {

void init_kernel_heap(void* start, std::size_t size);

} // namespace kernel::kinit

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

void* kimalloc(std::size_t size);
void kifree(void* ptr);

template <typename T>
struct ident_allocator {
    using value_type = T;
    using propagate_on_container_move_assignment = std::true_type;

    constexpr ident_allocator() = default;

    template <typename U>
    constexpr ident_allocator(const ident_allocator<U>&) noexcept {}
    
    inline T* allocate(std::size_t n)
    { return (T*)kimalloc(n * sizeof(T)); }
    inline void deallocate(T* ptr, std::size_t) { return kifree(ptr); }
};

template <typename T, typename... Args>
constexpr T* kinew(Args&&... args)
{
    ident_allocator<T> alloc { };
    T* ptr = std::allocator_traits<ident_allocator<T>>::allocate(alloc, 1);
    std::allocator_traits<ident_allocator<T>>::construct(alloc, ptr, std::forward<Args>(args)...);
    return ptr;
}

template <typename T>
constexpr void kidelete(T* ptr)
{
    ident_allocator<T> alloc { };
    std::allocator_traits<ident_allocator<T>>::destroy(alloc, ptr);
    std::allocator_traits<ident_allocator<T>>::deallocate(alloc, ptr, 1);
}

} // namespace types::memory
