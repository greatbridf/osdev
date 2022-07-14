#pragma once
#include <kernel/mem.h>
#include <types/cplusplus.hpp>
#include <types/stdint.h>
#include <types/types.h>

inline void* operator new(size_t, void* ptr)
{
    return ptr;
}

namespace types {

template <typename T>
concept Allocator = requires(size_t size, typename T::value_type* ptr)
{
    typename T::value_type;
    {
        T::allocate_memory(size)
        } -> same_as<typename T::value_type*>;
    {
        T::deallocate_memory(ptr)
        } -> same_as<void>;
};

template <Allocator T>
class allocator_traits;

template <typename T>
class kernel_allocator {
public:
    using value_type = T;

    static value_type* allocate_memory(size_t count)
    {
        return static_cast<value_type*>(::k_malloc(count));
    }

    static void deallocate_memory(value_type* ptr)
    {
        ::k_free(ptr);
    }
};

template <typename T>
class kernel_ident_allocator {
public:
    using value_type = T;

    static value_type* allocate_memory(size_t count)
    {
        return static_cast<value_type*>(::ki_malloc(count));
    }

    static void deallocate_memory(value_type* ptr)
    {
        ::ki_free(ptr);
    }
};

template <typename T, typename... Args>
constexpr T* kernel_allocator_new(Args&&... args)
{
    return allocator_traits<kernel_allocator<T>>::allocate_and_construct(forward<Args>(args)...);
}

template <typename T, typename... Args>
constexpr T* kernel_ident_allocator_new(Args&&... args)
{
    return allocator_traits<kernel_ident_allocator<T>>::allocate_and_construct(forward<Args>(args)...);
}

template <Allocator _allocator>
class allocator_traits {
public:
    using value_type = typename _allocator::value_type;

    static value_type* allocate(size_t count)
    {
        if (count == 0)
            return nullptr;
        return _allocator::allocate_memory(sizeof(value_type) * count);
    }

    template <typename... Args>
    static value_type* construct(value_type* ptr, Args&&... args)
    {
        new (ptr) value_type(forward<Args>(args)...);
        return ptr;
    }

    template <typename... Args>
    static value_type* allocate_and_construct(Args&&... args)
    {
        auto* ptr = allocate(1);
        construct(ptr, forward<Args>(args)...);
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
        _allocator::deallocate_memory(ptr);
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
