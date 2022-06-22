#pragma once
#include <kernel/mem.h>
#include <types/types.h>

inline void* operator new(size_t, void* ptr)
{
    return ptr;
}

namespace types {

template <typename Allocator>
class allocator_traits;

template <typename T>
class kernel_allocator {
public:
    using value_type = T;

    static value_type* allocate_memory(size_t count)
    {
        return static_cast<value_type*>(::k_malloc(sizeof(value_type) * count));
    }

    static void deallocate_memory(value_type* ptr)
    {
        ::k_free(ptr);
    }
};

template <typename T, typename... Args>
T* kernel_allocator_new(Args... args)
{
    return allocator_traits<kernel_allocator<T>>::allocate_and_construct(args...);
}

template <typename Allocator>
class allocator_traits {
public:
    using value_type = typename Allocator::value_type;

    static value_type* allocate(size_t count)
    {
        if (count == 0)
            return nullptr;
        return Allocator::allocate_memory(sizeof(value_type) * count);
    }

    template <typename... Args>
    static value_type* construct(value_type* ptr, Args... args)
    {
        new (ptr) value_type(args...);
        return ptr;
    }

    template <typename... Args>
    static value_type* allocate_and_construct(Args... args)
    {
        auto* ptr = allocate(1);
        construct(ptr, args...);
        return ptr;
    }

    static void deconstruct(value_type* ptr)
    {
        if (!ptr)
            return;
        ptr->~value_type();
    }

    static void deallocate(value_type* ptr)
    {
        if (!ptr)
            return;
        Allocator::deallocate_memory(ptr);
    }

    static void deconstruct_and_deallocate(value_type* ptr)
    {
        if (!ptr)
            return;
        deconstruct(ptr);
        deallocate(ptr);
    }
};
} // namespace types
