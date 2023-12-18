#pragma once
#include <new>
#include <utility>
#include <type_traits>
#include <bit>
#include <stdint.h>
#include <types/cplusplus.hpp>
#include <types/types.h>

namespace types {

namespace __allocator {
    class brk_memory_allocator {
    public:
        using byte = uint8_t;
        using size_type = size_t;

        struct mem_blk_flags {
            uint8_t is_free;
            uint8_t has_next;
            uint8_t _unused2;
            uint8_t _unused3;
        };

        struct mem_blk {
            size_t size;
            struct mem_blk_flags flags;
            // the first byte of the memory space
            // the minimal allocated space is 8 bytes
            byte data[];
        };

    private:
        byte* p_start;
        byte* p_break;
        byte* p_limit;

        brk_memory_allocator() = delete;
        brk_memory_allocator(const brk_memory_allocator&) = delete;
        brk_memory_allocator(brk_memory_allocator&&) = delete;

        constexpr byte* brk(byte* addr)
        {
            if (unlikely(addr >= p_limit))
                return nullptr;
            return p_break = addr;
        }

        constexpr byte* sbrk(size_type increment)
        { return brk(p_break + increment); }

        constexpr mem_blk* _next(mem_blk* blk, size_type blk_size)
        {
            auto* p = std::bit_cast<byte*>(blk);
            p += sizeof(mem_blk);
            p += blk_size;
            return std::bit_cast<mem_blk*>(p);
        }

        // blk MUST be free
        constexpr void unite_afterwards(mem_blk* blk)
        {
            while (blk->flags.has_next) {
                auto* blk_next = _next(blk, blk->size);
                if (!blk_next->flags.is_free)
                    break;
                blk->size += sizeof(mem_blk) + blk_next->size;
                blk->flags.has_next = blk_next->flags.has_next;
            }
        }

        // @param start_pos position where to start finding
        // @param size the size of the block we're looking for
        // @return found block if suitable block exists, if not, the last block
        constexpr mem_blk* find_blk(mem_blk* start_pos, size_type size)
        {
            while (true) {
                if (start_pos->flags.is_free) {
                    unite_afterwards(start_pos);

                    if (start_pos->size >= size)
                        break;
                }

                if (!start_pos->flags.has_next)
                    break;
                start_pos = _next(start_pos, start_pos->size);
            }
            return start_pos;
        }

        constexpr mem_blk* allocate_new_block(mem_blk* blk_before, size_type size)
        {
            auto ret = sbrk(sizeof(mem_blk) + size);
            if (!ret)
                return nullptr;

            mem_blk* blk = _next(blk_before, blk_before->size);

            blk_before->flags.has_next = 1;

            blk->flags.has_next = 0;
            blk->flags.is_free = 1;
            blk->size = size;

            return blk;
        }

        constexpr void split_block(mem_blk* blk, size_type this_size)
        {
            // block is too small to get split
            // that is, the block to be split should have enough room
            // for "this_size" bytes and also could contain a new block
            if (blk->size < this_size + sizeof(mem_blk) + 8)
                return;

            mem_blk* blk_next = _next(blk, this_size);

            blk_next->size = blk->size
                - this_size
                - sizeof(mem_blk);

            blk_next->flags.has_next = blk->flags.has_next;
            blk_next->flags.is_free = 1;

            blk->flags.has_next = 1;
            blk->size = this_size;
        }

    public:
        constexpr brk_memory_allocator(byte* start, size_type limit)
            : p_start(start)
            , p_limit(start + limit)
        {
            brk(p_start);
            auto* p_blk = std::bit_cast<mem_blk*>(sbrk(0));
            p_blk->size = 8;
            p_blk->flags.has_next = 0;
            p_blk->flags.is_free = 1;
        }

        constexpr void* alloc(size_type size)
        {
            // align to 8 bytes boundary
            size = (size + 7) & ~7;

            auto* block_allocated = find_blk(std::bit_cast<mem_blk*>(p_start), size);
            if (!block_allocated->flags.has_next
                && (!block_allocated->flags.is_free || block_allocated->size < size)) {
                // 'block_allocated' in the argument list is the pointer
                // pointing to the last block
                block_allocated = allocate_new_block(block_allocated, size);
                if (!block_allocated)
                    return nullptr;
            } else {
                split_block(block_allocated, size);
            }

            block_allocated->flags.is_free = 0;

            auto* blkpos = std::bit_cast<byte*>(block_allocated);
            if (blkpos > p_start)
                p_start = blkpos;
            return block_allocated->data;
        }

        constexpr void free(void* ptr)
        {
            auto* blk = std::bit_cast<mem_blk*>(
                std::bit_cast<byte*>(ptr) - sizeof(mem_blk));

            blk->flags.is_free = 1;

            if (std::bit_cast<byte*>(blk) < p_start)
                p_start = std::bit_cast<byte*>(blk);

            // unite free blocks nearby
            unite_afterwards(blk);
        }
    };
}; // namespace __allocator

template <typename T>
concept Allocator = requires(size_t size, typename T::value_type* ptr)
{
    typename T::value_type;
    {
        T::allocate_memory(size)
    };
    {
        T::deallocate_memory(ptr)
    };
    std::is_same_v<typename T::value_type*, decltype(T::allocate_memory(size))>;
    std::is_same_v<void, decltype(T::deallocate_memory(ptr))>;
};

template <Allocator T>
class allocator_traits;

namespace __allocator {
    inline char __ident_heap[0x100000];
    inline __allocator::brk_memory_allocator
        m_alloc { (uint8_t*)__ident_heap, sizeof(__ident_heap) };
} // namespace __allocator

template <typename T>
class kernel_ident_allocator {
public:
    using value_type = T;

    static constexpr value_type* allocate_memory(size_t count)
    {
        return static_cast<value_type*>(__allocator::m_alloc.alloc(count));
    }

    static constexpr void deallocate_memory(value_type* ptr)
    {
        __allocator::m_alloc.free(ptr);
    }
};

template <template <typename _T> class Allocator, typename T, typename... Args>
constexpr T* _new(Args&&... args)
{
    return allocator_traits<Allocator<T>>::allocate_and_construct(std::forward<Args>(args)...);
}

template <template <typename _T> class Allocator, typename T, typename... Args>
constexpr T* pnew(T* = nullptr, Args&&... args)
{
    return _new<Allocator, T, Args...>(std::forward<Args>(args)...);
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
        new (ptr) value_type(std::forward<Args>(args)...);
        return ptr;
    }

    template <typename... Args>
    static constexpr value_type* allocate_and_construct(Args&&... args)
    {
        auto* ptr = allocate(1);
        construct(ptr, std::forward<Args>(args)...);
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

namespace __allocator {
    inline __allocator::brk_memory_allocator* m_palloc;
    inline void init_kernel_heap(void* start, size_t sz)
    {
        m_palloc = pnew<kernel_ident_allocator>(m_palloc, (uint8_t*)start, sz);
    }
} // namespace __allocator

template <typename T>
class kernel_allocator {
public:
    using value_type = T;

    static constexpr value_type* allocate_memory(size_t count)
    {
        return static_cast<value_type*>(__allocator::m_palloc->alloc(count));
    }

    static constexpr void deallocate_memory(value_type* ptr)
    {
        __allocator::m_palloc->free(ptr);
    }
};

template <typename T, template <typename> typename Allocator>
struct allocator_adapter {
    using value_type = typename Allocator<T>::value_type;
    using propagate_on_container_move_assignment = std::true_type;

    constexpr allocator_adapter() = default;

    template <template <typename> typename UAlloc, typename U>
    constexpr allocator_adapter(const allocator_adapter<U, UAlloc>&)
        noexcept {}
    
    constexpr T* allocate(std::size_t n)
    { return types::allocator_traits<Allocator<T>>::allocate(n); }
    constexpr void deallocate(T* ptr, std::size_t)
    { return types::allocator_traits<Allocator<T>>::deallocate(ptr); }

    template <typename U>
    struct rebind { using other = allocator_adapter<U, Allocator>; };
};

} // namespace types
