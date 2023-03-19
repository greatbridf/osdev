#pragma once

#include <stdint.h>
#include <stdio.h>
#include <types/allocator.hpp>

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
    constexpr char _get_char(char* ptr)
    {
        --count;
        return *ptr;
    }

    constexpr void _put_char(char c)
    {
        *head = c;
        ++count;
    }

    constexpr char* _forward(char* ptr)
    {
        if (ptr == end)
            return start;
        else
            return ptr + 1;
    }

    constexpr char* _backward(char* ptr)
    {
        if (ptr == start)
            return end;
        else
            return ptr - 1;
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

    constexpr int front(void)
    {
        if (empty())
            return EOF;
        return *base;
    }

    constexpr int back(void)
    {
        if (empty())
            return EOF;
        return *_backward(head);
    }

    constexpr int get(void)
    {
        if (empty())
            return EOF;

        char c = _get_char(base);
        base = _forward(base);
        return c;
    }

    constexpr int pop(void)
    {
        if (empty())
            return EOF;

        char c = _get_char(_backward(head));
        head = _backward(head);
        return c;
    }

    constexpr int put(char c)
    {
        if (full())
            return EOF;

        _put_char(c);
        head = _forward(head);
        return c;
    }
};

} // namespace types
