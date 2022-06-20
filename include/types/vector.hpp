#pragma once

#include <kernel/mem.h>
#include <types/allocator.hpp>
#include <types/types.h>

namespace types {

template <typename T, template <typename _value_type> class Allocator = kernel_allocator>
class vector {
public:
    class iterator;

    using value_type = T;
    using pointer_type = value_type*;
    using reference_type = value_type&;
    using iterator_type = iterator;
    using size_type = size_t;
    using difference_type = ssize_t;
    using index_type = size_type;
    using allocator_type = Allocator<value_type>;

public:
    class iterator {
    public:
        explicit iterator(const iterator& iter) noexcept
            : p(iter.p)
        {
        }

        explicit iterator(iterator&& iter) noexcept
            : p(iter.p)
        {
        }

        explicit iterator(pointer_type p) noexcept
            : p(p)
        {
        }

        bool operator==(const iterator& iter) noexcept
        {
            return this->p == iter.p;
        }

        bool operator!=(const iterator& iter) noexcept
        {
            return !(*this == iter);
        }

        iterator& operator++() noexcept
        {
            ++p;
            return *this;
        }

        iterator operator++(int) noexcept
        {
            iterator iter(*this);
            ++p;
            return iter;
        }

        iterator& operator--() noexcept
        {
            --p;
            return *this;
        }

        iterator operator--(int) noexcept
        {
            iterator iter(*this);
            --p;
            return iter;
        }

        reference_type operator*() const noexcept
        {
            return *p;
        }

        pointer_type operator->() const noexcept
        {
            return p;
        }

    protected:
        pointer_type p;
    };

public:
    vector() noexcept
        : m_size(0)
    {
        resize(1);
    }

    ~vector() noexcept
    {
        resize(0);
    }

    void resize(size_type n)
    {
        value_type* new_ptr = allocator_traits<allocator_type>::allocate(n);

        m_capacity = n;
        size_t orig_size = m_size;
        if (m_size > m_capacity)
            m_size = m_capacity;

        for (size_t i = 0; i < m_size; ++i)
            allocator_traits<allocator_type>::construct(new_ptr + i, _at(i));

        for (size_t i = 0; i < orig_size; ++i)
            allocator_traits<allocator_type>::deconstruct(m_arr + i);

        allocator_traits<allocator_type>::deallocate(m_arr);
        m_arr = new_ptr;
    }

    // TODO: find

    // erase the node which iter points to
    void erase(const iterator_type& iter) noexcept
    {
        allocator_traits<allocator_type>::deconstruct(iter.p);
        --m_size;
    }

    // insert the value v in front of the given iterator
    // void insert(const iterator_type& iter, const value_type& v) noexcept
    // {
    //     node_base_type* new_node = allocator_traits<allocator_type>::allocate(v);
    //     iter._node()->prev->connect(new_node);
    //     new_node->connect(iter._node());

    //     ++_size();
    // }

    // insert the value v in front of the given iterator
    // void insert(const iterator_type& iter, value_type&& v) noexcept
    // {
    //     node_base_type* new_node = allocator_traits<allocator_type>::allocate(v);
    //     iter._node().prev->connect(new_node);
    //     new_node->connect(iter._node());

    //     ++_size();
    // }

    value_type* data(void) noexcept
    {
        return m_arr;
    }

    const value_type* data(void) const noexcept
    {
        return m_arr;
    }

    value_type& at(index_type i) noexcept
    {
        // TODO: boundary check
        return _at(i);
    }

    const value_type& at(index_type i) const noexcept
    {
        // TODO: boundary check
        return _at(i);
    }

    value_type& operator[](index_type i) noexcept
    {
        return at(i);
    }

    const value_type& operator[](index_type i) const noexcept
    {
        return at(i);
    }

    void push_back(const value_type& v) noexcept
    {
        if (m_size == m_capacity)
            resize(m_capacity * 2);
        allocator_traits<allocator_type>::construct(m_arr + m_size, v);
        ++m_size;
    }

    void push_back(value_type&& v) noexcept
    {
        if (m_size == m_capacity)
            resize(m_capacity * 2);
        allocator_traits<allocator_type>::construct(m_arr + m_size, v);
        ++m_size;
    }

    size_t size(void) const noexcept
    {
        return m_size;
    }

    iterator_type begin() noexcept
    {
        return iterator_type(m_arr);
    }

    iterator_type end() noexcept
    {
        return iterator_type(m_arr + m_size - 1);
    }

    bool empty(void) const noexcept
    {
        return size() == 0;
    }

    // TODO
    // iterator_type cstart() noexcept;
    // iterator_type cend() noexcept;

    // iterator_type r_start() noexcept;
    // iterator_type r_end() noexcept;

    // iterator_type cr_start() noexcept;
    // iterator_type cr_end() noexcept;

private:
    inline value_type& _at(index_type i)
    {
        return m_arr[i];
    }

private:
    value_type* m_arr;
    size_type m_capacity;
    size_type m_size;
};

} // namespace types
