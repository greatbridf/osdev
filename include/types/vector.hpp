#pragma once
#include <utility>
#include <type_traits>

#include <assert.h>
#include <types/allocator.hpp>
#include <types/types.h>

namespace types {

template <typename T, template <typename _value_type> class Allocator = kernel_allocator>
class vector {
public:
    template <typename Pointer>
    class iterator;

    using value_type = T;
    using pointer_type = std::add_pointer_t<value_type>;
    using reference_type = std::add_lvalue_reference_t<value_type>;
    using iterator_type = iterator<pointer_type>;

    using const_value_type = std::add_const_t<value_type>;
    using const_pointer_type = std::add_pointer_t<const_value_type>;
    using const_reference_type = std::add_lvalue_reference_t<const_value_type>;
    using const_iterator_type = iterator<const_pointer_type>;

    using size_type = size_t;
    using difference_type = ssize_t;
    using index_type = size_type;
    using allocator_type = Allocator<value_type>;

public:
    template <typename Pointer>
    class iterator {
    public:
        using Value = std::remove_pointer_t<Pointer>;
        using Reference = std::add_lvalue_reference_t<Value>;

    public:
        explicit constexpr iterator(Pointer p) noexcept
            : p(p) {}

        constexpr iterator(const iterator& iter) noexcept
            : p(iter.p) {}
        constexpr iterator(iterator&& iter) noexcept
            : p(std::exchange(iter.p, nullptr)) {}

        constexpr iterator& operator=(const iterator& iter)
        {
            p = iter.p;
            return *this;
        }
        constexpr iterator& operator=(iterator&& iter)
        {
            p = std::exchange(iter.p, nullptr);
            return *this;
        }

        constexpr bool operator==(const iterator& iter) const noexcept
        { return this->p == iter.p; }
        constexpr bool operator!=(const iterator& iter) const noexcept
        { return !operator==(iter); }

        constexpr iterator& operator++(void) noexcept
        { ++p; return *this; }
        constexpr iterator operator++(int) noexcept
        { iterator iter(*this); ++p; return iter; }

        constexpr iterator& operator--(void) noexcept
        { --p; return *this; }
        constexpr iterator operator--(int) noexcept
        { iterator iter(*this); --p; return iter; }

        constexpr iterator operator+(size_type n) noexcept
        { return iterator { p + n }; }
        constexpr iterator operator-(size_type n) noexcept
        { return iterator { p - n }; }

        constexpr Reference operator*(void) const noexcept
        { return *p; }
        constexpr Pointer operator&(void) const noexcept
        { return p; }
        constexpr Pointer operator->(void) const noexcept
        { return p; }

    protected:
        Pointer p;
    };

protected:
    constexpr const value_type& _at(index_type i) const
    { return m_arr[i]; }
    constexpr value_type& _at(index_type i)
    { return m_arr[i]; }

    // assert(n >= m_size)
    constexpr void _reallocate_safe(size_type n)
    {
        auto* newptr = allocator_traits<allocator_type>::allocate(n);
        for (size_t i = 0; i < m_size; ++i) {
            allocator_traits<allocator_type>::construct(newptr + i, std::move(_at(i)));
            allocator_traits<allocator_type>::deconstruct(m_arr + i);
        }

        allocator_traits<allocator_type>::deallocate(m_arr);
        m_arr = newptr;
        m_capacity = n;
    }

    // make m_capacity >= n >= m_size
    constexpr void _pre_resize(size_type n)
    {
        if (n == m_size)
            return;

        if (n < m_size) {
            while (n < m_size)
                pop_back();
            return;
        }
        assert(n > m_size);
        reserve(n);
    }

public:
    constexpr vector() noexcept
        : m_arr(nullptr)
        , m_capacity(0)
        , m_size(0) { }

    explicit constexpr vector(size_type size)
        : vector()
    { resize(size); }

    constexpr vector(size_type size, const value_type& value)
        : vector()
    { resize(size, value); }

    constexpr vector(const vector& arr) noexcept
        : vector()
    {
        for (const auto& item : arr)
            push_back(item);
    }

    constexpr vector(vector&& arr) noexcept
    {
        m_arr = std::exchange(arr.m_arr, nullptr);
        m_capacity = std::exchange(arr.m_capacity, 0);
        m_size = std::exchange(arr.m_size, 0);
    }

    constexpr vector& operator=(vector&& arr)
    {
        resize(0);
        shrink_to_fit();

        m_arr = std::exchange(arr.m_arr, nullptr);
        m_capacity = std::exchange(arr.m_capacity, 0);
        m_size = std::exchange(arr.m_size, 0);

        return *this;
    }

    constexpr vector& operator=(const vector& arr)
    {
        return operator=(vector {arr});
    }

    constexpr ~vector() noexcept
    {
        resize(0);
        shrink_to_fit();
    }

    constexpr void shrink_to_fit()
    {
        if (m_size == m_capacity)
            return;

        _reallocate_safe(m_size);
    }

    constexpr void reserve(size_type n)
    {
        if (n <= m_capacity)
            return;

        _reallocate_safe(n);
    }

    constexpr void resize(size_type n)
    {
        _pre_resize(n);
        while (n > m_size)
            emplace_back();
    }

    constexpr void resize(size_type n, const value_type& value)
    {
        _pre_resize(n);
        while (n > m_size)
            emplace_back(value);
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
    { return m_arr; }
    constexpr const value_type* data(void) const noexcept
    { return m_arr; }

    constexpr value_type& at(index_type i) noexcept
    {
        assert(i + 1 <= this->size());
        return _at(i);
    }
    constexpr const value_type& at(index_type i) const noexcept
    {
        assert(i + 1 <= this->size());
        return _at(i);
    }

    constexpr value_type& operator[](index_type i) noexcept
    { return at(i); }
    constexpr const value_type& operator[](index_type i) const noexcept
    { return at(i); }

    constexpr void push_back(const value_type& v) noexcept
    {
        if (m_size == m_capacity)
            reserve(m_capacity ? m_capacity * 2 : 1);
        allocator_traits<allocator_type>::construct(m_arr + m_size, v);
        ++m_size;
    }

    constexpr void push_back(value_type&& v) noexcept
    {
        if (m_size == m_capacity)
            reserve(m_capacity ? m_capacity * 2 : 1);
        allocator_traits<allocator_type>::construct(m_arr + m_size, std::move(v));
        ++m_size;
    }

    template <typename... Args>
    constexpr iterator_type emplace_back(Args&&... args)
    {
        push_back(value_type(std::forward<Args>(args)...));
        return iterator_type(m_arr + m_size - 1);
    }

    constexpr void pop_back(void) noexcept
    {
        assert(m_size > 0);
        allocator_traits<allocator_type>::deconstruct(m_arr + m_size - 1);
        --m_size;
    }

    constexpr size_type size(void) const noexcept
    { return m_size; }

    constexpr size_type capacity(void) const noexcept
    { return m_capacity; }

    constexpr const_iterator_type cbegin() const noexcept
    { return const_iterator_type(m_arr); }

    constexpr const_iterator_type cend() const noexcept
    { return const_iterator_type(m_arr + m_size); }

    constexpr iterator_type begin() noexcept
    { return iterator_type(m_arr); }
    constexpr const_iterator_type begin() const noexcept
    { return cbegin(); }

    constexpr iterator_type end() noexcept
    { return iterator_type(m_arr + m_size); }
    constexpr const_iterator_type end() const noexcept
    { return cend(); }

    constexpr value_type& back() noexcept
    {
        assert(m_size != 0);
        return at(m_size - 1);
    }
    constexpr const value_type& back() const noexcept
    {
        assert(m_size != 0);
        return at(m_size - 1);
    }

    constexpr bool empty(void) const noexcept
    { return m_size == 0; }

    constexpr void clear(void)
    { resize(0); }

    // TODO

    // iterator_type r_start() noexcept;
    // iterator_type r_end() noexcept;

    // iterator_type cr_start() noexcept;
    // iterator_type cr_end() noexcept;

protected:
    T* m_arr;
    size_type m_capacity;
    size_type m_size;
};

} // namespace types
