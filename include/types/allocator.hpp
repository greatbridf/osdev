#pragma once
#include <kernel/mem.h>
#include <stdint.h>
#include <types/cplusplus.hpp>
#include <types/types.h>

constexpr void* operator new(size_t, void* ptr)
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

    static constexpr value_type* allocate_memory(size_t count)
    {
        return static_cast<value_type*>(::k_malloc(count));
    }

    static constexpr void deallocate_memory(value_type* ptr)
    {
        ::k_free(ptr);
    }
};

template <typename T>
class kernel_ident_allocator {
public:
    using value_type = T;

    static constexpr value_type* allocate_memory(size_t count)
    {
        return static_cast<value_type*>(::ki_malloc(count));
    }

    static constexpr void deallocate_memory(value_type* ptr)
    {
        ::ki_free(ptr);
    }
};

template <template <typename _T> class Allocator, typename T, typename... Args>
constexpr T* _new(Args&&... args)
{
    return allocator_traits<Allocator<T>>::allocate_and_construct(forward<Args>(args)...);
}

template <template <typename _T> class Allocator, typename T, typename... Args>
constexpr T* pnew(T* = nullptr, Args&&... args)
{
    return _new<Allocator, T, Args...>(forward<Args>(args)...);
}

template <template <typename _T> class Allocator, typename T>
constexpr void pdelete(T* ptr)
{
    allocator_traits<Allocator<T>>::deconstruct_and_deallocate(ptr);
}

template <Allocator _allocator>
class allocator_traits {
public:
    using value_type = typename _allocator::value_type;

    static constexpr value_type* allocate(size_t count)
    {
        if (count == 0)
            return nullptr;
        return _allocator::allocate_memory(sizeof(value_type) * count);
    }

    template <typename... Args>
    static constexpr value_type* construct(value_type* ptr, Args&&... args)
    {
        new (ptr) value_type(forward<Args>(args)...);
        return ptr;
    }

    template <typename... Args>
    static constexpr value_type* allocate_and_construct(Args&&... args)
    {
        auto* ptr = allocate(1);
        construct(ptr, forward<Args>(args)...);
        return ptr;
    }

    static constexpr void deconstruct(value_type* ptr)
    {
        if (!ptr)
            return;
        ptr->~value_type();
    }

    static constexpr void deallocate(value_type* ptr)
    {
        if (!ptr)
            return;
        _allocator::deallocate_memory(ptr);
    }

    static constexpr void deconstruct_and_deallocate(value_type* ptr)
    {
        if (!ptr)
            return;
        deconstruct(ptr);
        deallocate(ptr);
    }
};
} // namespace types
