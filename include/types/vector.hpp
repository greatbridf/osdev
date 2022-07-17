#pragma once

#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/types.h>

namespace types {

template <typename T, template <typename _value_type> class Allocator = kernel_allocator>
class vector {
public:
    template <typename Pointer>
    class iterator;

    using value_type = T;
    using pointer_type = value_type*;
    using reference_type = value_type&;
    using iterator_type = iterator<value_type*>;
    using const_iterator_type = iterator<const value_type*>;
    using size_type = size_t;
    using difference_type = ssize_t;
    using index_type = size_type;
    using allocator_type = Allocator<value_type>;

public:
    template <typename Pointer>
    class iterator {
    public:
        using Value = typename types::traits::remove_pointer<Pointer>::type;
        using Reference = typename types::traits::add_reference<Value>::type;

    public:
        constexpr iterator(const iterator& iter) noexcept
            : p(iter.p)
        {
        }

        constexpr iterator(iterator&& iter) noexcept
            : p(iter.p)
        {
            iter.p = nullptr;
        }

        constexpr iterator& operator=(const iterator& iter)
        {
            p = iter.p;
            return *this;
        }

        explicit constexpr iterator(Pointer p) noexcept
            : p(p)
        {
        }

        constexpr bool operator==(const iterator& iter) const noexcept
        {
            return this->p == iter.p;
        }

        constexpr bool operator!=(const iterator& iter) const noexcept
        {
            return !(*this == iter);
        }

        constexpr iterator& operator++() noexcept
        {
            ++p;
            return *this;
        }

        constexpr iterator operator++(int) noexcept
        {
            iterator iter(*this);
            ++p;
            return iter;
        }

        constexpr iterator& operator--() noexcept
        {
            --p;
            return *this;
        }

        constexpr iterator operator--(int) noexcept
        {
            iterator iter(*this);
            --p;
            return iter;
        }

        constexpr iterator operator+(size_type n) noexcept
        {
            iterator iter(p + n);
            return iter;
        }

        constexpr iterator operator-(size_type n) noexcept
        {
            iterator iter(p - n);
            return iter;
        }

        constexpr Reference operator*() const noexcept
        {
            return *p;
        }

        constexpr Pointer operator->() const noexcept
        {
            return p;
        }

    protected:
        Pointer p;
    };

public:
    explicit constexpr vector(size_type capacity = 1) noexcept
        : m_arr(nullptr)
        , m_size(0)
    {
        resize(capacity);
    }

    constexpr vector(const vector& arr) noexcept
        : vector(arr.capacity())
    {
        for (const auto& item : arr)
            push_back(item);
    }

    constexpr vector(vector&& arr) noexcept
    {
        m_arr = arr.m_arr;
        m_capacity = arr.m_capacity;
        m_size = arr.m_size;

        arr.m_arr = nullptr;
        arr.m_capacity = 0;
        arr.m_size = 0;
    }

    constexpr vector& operator=(vector&& arr)
    {
        resize(0);
        m_arr = arr.m_arr;
        m_capacity = arr.m_capacity;
        m_size = arr.m_size;

        arr.m_arr = nullptr;
        arr.m_capacity = 0;
        arr.m_size = 0;

        return *this;
    }

    constexpr vector& operator=(const vector& arr)
    {
        return operator=(vector(arr));
    }

    ~vector() noexcept
    {
        resize(0);
    }

    constexpr void resize(size_type n)
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

        if (m_arr)
            allocator_traits<allocator_type>::deallocate(m_arr);
        m_arr = new_ptr;
    }

    // TODO: find

    // erase the node which iter points to
    // void erase(const iterator_type& iter) noexcept
    // {
    //     allocator_traits<allocator_type>::deconstruct(iter.p);
    //     --m_size;
    // }

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

    constexpr value_type* data(void) noexcept
    {
        return m_arr;
    }

    constexpr const value_type* data(void) const noexcept
    {
        return m_arr;
    }

    constexpr value_type& at(index_type i) noexcept
    {
        // TODO: boundary check
        return _at(i);
    }

    constexpr const value_type& at(index_type i) const noexcept
    {
        // TODO: boundary check
        return _at(i);
    }

    constexpr value_type& operator[](index_type i) noexcept
    {
        return at(i);
    }

    constexpr const value_type& operator[](index_type i) const noexcept
    {
        return at(i);
    }

    constexpr void push_back(const value_type& v) noexcept
    {
        if (m_size == m_capacity)
            resize(m_capacity * 2);
        allocator_traits<allocator_type>::construct(m_arr + m_size, v);
        ++m_size;
    }

    constexpr void push_back(value_type&& v) noexcept
    {
        if (m_size == m_capacity)
            resize(m_capacity * 2);
        allocator_traits<allocator_type>::construct(m_arr + m_size, move(v));
        ++m_size;
    }

    template <typename... Args>
    constexpr iterator_type emplace_back(Args&&... args)
    {
        push_back(value_type(forward<Args>(args)...));
        return back();
    }

    constexpr void pop_back(void) noexcept
    {
        allocator_traits<allocator_type>::deconstruct(&*back());
        --m_size;
    }

    constexpr size_type size(void) const noexcept
    {
        return m_size;
    }

    constexpr size_type capacity(void) const noexcept
    {
        return m_capacity;
    }

    constexpr const_iterator_type cbegin() const noexcept
    {
        return const_iterator_type(m_arr);
    }

    constexpr const_iterator_type cend() const noexcept
    {
        return const_iterator_type(m_arr + m_size);
    }

    constexpr iterator_type begin() noexcept
    {
        return iterator_type(m_arr);
    }

    constexpr const_iterator_type begin() const noexcept
    {
        return cbegin();
    }

    constexpr iterator_type end() noexcept
    {
        return iterator_type(m_arr + m_size);
    }

    constexpr const_iterator_type end() const noexcept
    {
        return cend();
    }

    constexpr iterator_type back() noexcept
    {
        return iterator_type(m_arr + m_size - 1);
    }

    constexpr const_iterator_type back() const noexcept
    {
        return const_iterator_type(m_arr + m_size - 1);
    }

    constexpr bool empty(void) const noexcept
    {
        return size() == 0;
    }

    constexpr void clear(void)
    {
        for (size_t i = 0; i < size(); ++i)
            allocator_traits<allocator_type>::deconstruct(m_arr + i);
        m_size = 0;
    }

    // TODO

    // iterator_type r_start() noexcept;
    // iterator_type r_end() noexcept;

    // iterator_type cr_start() noexcept;
    // iterator_type cr_end() noexcept;

protected:
    inline constexpr const value_type& _at(index_type i) const
    {
        return m_arr[i];
    }
    inline constexpr value_type& _at(index_type i)
    {
        return m_arr[i];
    }

protected:
    T* m_arr;
    size_type m_capacity;
    size_type m_size;
};

} // namespace types
