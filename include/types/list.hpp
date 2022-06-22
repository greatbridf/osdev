#pragma once

#include <kernel/mem.hpp>
#include <types/allocator.hpp>
#include <types/types.h>

namespace types {

template <typename T, template <typename _value_type> class Allocator = kernel_allocator>
class list {
private:
    class node_base;
    template <typename NodeValue>
    class node;

public:
    class iterator;

    using value_type = T;
    using pointer_type = value_type*;
    using reference_type = value_type&;
    using iterator_type = iterator;
    using size_type = size_t;
    using difference_type = ssize_t;
    using node_base_type = node_base;
    using node_type = node<value_type>;
    using allocator_type = Allocator<node_type>;
    using sentry_node_type = node<size_t>;
    using sentry_allocator_type = Allocator<sentry_node_type>;

private:
    class node_base {
    public:
        node_base* prev = 0;
        node_base* next = 0;

        void connect(node_base* _next) noexcept
        {
            this->next = _next;
            _next->prev = this;
        }
    };

    template <typename NodeValue>
    class node : public node_base {
    public:
        explicit node(const NodeValue& v) noexcept
            : value(v)
        {
        }

        explicit node(NodeValue&& v) noexcept
            : value(v)
        {
        }

        NodeValue value;
    };

public:
    class iterator {
    public:
        iterator(const iterator& iter) noexcept
            : n(iter.n)
        {
        }

        iterator(iterator&& iter) noexcept
            : n(iter.n)
        {
        }

        explicit iterator(node_base* _n) noexcept
            : n(_n)
        {
        }

        bool operator==(const iterator& iter) noexcept
        {
            return this->_node() == iter._node();
        }

        bool operator!=(const iterator& iter) noexcept
        {
            return !(*this == iter);
        }

        iterator& operator++() noexcept
        {
            n = n->next;
            return *this;
        }

        iterator operator++(int) noexcept
        {
            iterator iter(*this);
            n = n->next;
            return iter;
        }

        iterator& operator--() noexcept
        {
            n = n->prev;
            return *this;
        }

        iterator operator--(int) noexcept
        {
            iterator iter(*this);
            n = n->prev;
            return iter;
        }

        reference_type operator*() const noexcept
        {
            return (static_cast<node_type*>(n))->value;
        }

        pointer_type operator->() const noexcept
        {
            return &(static_cast<node_type*>(n))->value;
        }

        pointer_type ptr(void) const noexcept
        {
            return &(static_cast<node_type*>(n))->value;
        }

        node_base_type* _node(void) const noexcept
        {
            return n;
        }

    protected:
        node_base_type* n;
    };

private:
    node_base_type* head;
    node_base_type* tail;

    const size_t& _size(void) const noexcept
    {
        return (static_cast<sentry_node_type*>(head))->value;
    }

    size_t& _size(void) noexcept
    {
        return (static_cast<sentry_node_type*>(head))->value;
    }

public:
    list() noexcept
        // size is stored in the 'head' node
        : head(allocator_traits<sentry_allocator_type>::allocate_and_construct(0))
        , tail(allocator_traits<sentry_allocator_type>::allocate_and_construct(0))
    {
        head->connect(tail);
        tail->connect(head);
    }

    ~list() noexcept
    {
        for (auto iter = begin(); iter != end(); ++iter) {
            erase(iter);
        }
        allocator_traits<sentry_allocator_type>::deconstruct_and_deallocate(static_cast<sentry_node_type*>(head));
        allocator_traits<sentry_allocator_type>::deconstruct_and_deallocate(static_cast<sentry_node_type*>(tail));
    }

    iterator_type find(const value_type& v) noexcept
    {
        for (iterator_type iter = begin(); iter != end(); ++iter)
            if (*iter == v)
                return iter;
    }

    // erase the node which iter points to
    void erase(const iterator_type& iter) noexcept
    {
        node_base_type* current_node = iter._node();
        current_node->prev->connect(current_node->next);
        allocator_traits<allocator_type>::deconstruct_and_deallocate(static_cast<node_type*>(current_node));
        --_size();
    }

    // insert the value v in front of the given iterator
    void insert(const iterator_type& iter, const value_type& v) noexcept
    {
        node_base_type* new_node = allocator_traits<allocator_type>::allocate_and_construct(v);
        iter._node()->prev->connect(new_node);
        new_node->connect(iter._node());

        ++_size();
    }

    // insert the value v in front of the given iterator
    void insert(const iterator_type& iter, value_type&& v) noexcept
    {
        node_base_type* new_node = allocator_traits<allocator_type>::allocate_and_construct(v);
        iter._node().prev->connect(new_node);
        new_node->connect(iter._node());

        ++_size();
    }

    void push_back(const value_type& v) noexcept
    {
        insert(end(), v);
    }

    void push_back(value_type&& v) noexcept
    {
        insert(end(), v);
    }

    void push_front(const value_type& v) noexcept
    {
        insert(begin(), v);
    }

    void push_front(value_type&& v) noexcept
    {
        insert(begin(), v);
    }

    size_t size(void) const noexcept
    {
        return _size();
    }

    iterator_type begin() noexcept
    {
        return iterator_type(head->next);
    }

    iterator_type end() noexcept
    {
        return iterator_type(tail);
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
};

} // namespace types
