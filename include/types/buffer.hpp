#pragma once

#include <types/allocator.hpp>
#include <types/stdint.h>

namespace types {

template <template <typename> class Allocator>
class buffer {
public:
    using allocator_type = Allocator<char>;

private:
    char* const start;
    char* const end;
    char* base;
    char* head;
    size_t count;

private:
    constexpr char _get_char(void)
    {
        --count;
        return *base;
    }

    constexpr void _put_char(char c)
    {
        *head = c;
        ++count;
    }

    constexpr void _forward(char*& ptr)
    {
        if (ptr == end)
            ptr = start;
        else
            ++ptr;
    }

public:
    constexpr buffer(size_t size)
        : start { types::allocator_traits<allocator_type>::allocate(size) }
        , end { start + size - 1 }
        , base { start }
        , head { start }
        , count { 0 }
    {
    }

    constexpr buffer(const buffer& buf)
        : start { types::allocator_traits<allocator_type>::allocate(buf.end + 1 - buf.start) }
        , end { (uint32_t)start + (uint32_t)buf.end - (uint32_t)buf.start }
        , base { (uint32_t)start + (uint32_t)buf.base - (uint32_t)buf.start }
        , head { (uint32_t)start + (uint32_t)buf.base - (uint32_t)buf.start }
        , count { buf.count }
    {
    }

    constexpr buffer(buffer&& buf)
        : start { buf.start }
        , end { buf.end }
        , base { buf.base }
        , head { buf.head }
        , count { buf.count }
    {
    }

    constexpr ~buffer()
    {
        if (start)
            types::allocator_traits<allocator_type>::deallocate(start);
    }

    constexpr bool empty(void) const
    {
        return count == 0;
    }

    constexpr bool full(void) const
    {
        return count == static_cast<size_t>(end - start + 1);
    }

    constexpr char get(void)
    {
        // TODO: set error flag
        if (empty())
            return 0xff;

        char c = _get_char();
        _forward(base);
        return c;
    }

    constexpr char put(char c)
    {
        // TODO: set error flag
        if (full())
            return 0xff;

        _put_char(c);
        _forward(head);
        return c;
    }
};

} // namespace types
