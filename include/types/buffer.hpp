#pragma once

#include <memory>

#include <stdint.h>
#include <stdio.h>
#include <types/allocator.hpp>

namespace types {

template <typename Allocator>
class basic_buffer {
public:
    using alloc_traits = std::allocator_traits<Allocator>;

private:
    char* const start;
    char* const end;
    char* base;
    char* head;
    size_t count;
    Allocator alloc { };

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
    constexpr basic_buffer(size_t size)
        : start { alloc_traits::allocate(alloc, size) }
        , end { start + size - 1 }
        , base { start }
        , head { start }
        , count { 0 }
    {
    }

    constexpr basic_buffer(const basic_buffer& buf)
        : start { alloc_traits::allocate(alloc, buf.end + 1 - buf.start) }
        , end { (uint32_t)start + (uint32_t)buf.end - (uint32_t)buf.start }
        , base { (uint32_t)start + (uint32_t)buf.base - (uint32_t)buf.start }
        , head { (uint32_t)start + (uint32_t)buf.base - (uint32_t)buf.start }
        , count { buf.count }
    {
    }

    constexpr basic_buffer(basic_buffer&& buf)
        : start { buf.start }
        , end { buf.end }
        , base { buf.base }
        , head { buf.head }
        , count { buf.count }
    {
    }

    constexpr ~basic_buffer()
    {
        if (start)
            alloc_traits::deallocate(alloc, start, end - start);
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

    constexpr size_t size(void) const
    {
        return count;
    }

    constexpr size_t avail(void) const
    {
        return end - start + 1 - count;
    }

    constexpr void clear(void)
    {
        count = 0;
        head = base;
    }
};

using buffer = basic_buffer<std::allocator<char>>;

} // namespace types
