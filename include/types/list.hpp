#pragma once

#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/types.h>

namespace types {

template <typename T, template <typename _value_type> class Allocator = kernel_allocator>
class list {
private:
    class node_base;
    template <typename NodeValue>
    class node;

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
    using node_base_type = node_base;
    using node_type = node<value_type>;
    using allocator_type = Allocator<node_type>;
    using sentry_node_type = node<size_t>;
    using sentry_allocator_type = Allocator<sentry_node_type>;

private:
    class node_base {
    public:
        node_type* prev = 0;
        node_type* next = 0;

        void connect(node_type* _next) noexcept
        {
            this->next = _next;
            _next->prev = static_cast<node_type*>(this);
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
            : value(move(v))
        {
        }

        NodeValue value;
    };

public:
    template <typename Pointer>
    class iterator {
    public:
        using Value = typename types::traits::remove_pointer<Pointer>::type;
        using Reference = typename types::traits::add_reference<Value>::type;

    public:
        iterator(const iterator& iter) noexcept
            : n(iter.n)
        {
        }

        iterator(iterator&& iter) noexcept
            : n(iter.n)
        {
            iter.n = nullptr;
        }

        iterator& operator=(const iterator& iter)
        {
            n = iter.n;
            return *this;
        }

        explicit iterator(node_type* _n) noexcept
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

        Reference operator*() const noexcept
        {
            return n->value;
        }

        Pointer operator->() const noexcept
        {
            return &n->value;
        }

        Pointer ptr(void) const noexcept
        {
            return &n->value;
        }

        node_base_type* _node(void) const noexcept
        {
            return n;
        }

    protected:
        node_type* n;
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

    void destroy(void)
    {
        if (!head || !tail)
            return;
        clear();
        allocator_traits<sentry_allocator_type>::deconstruct_and_deallocate(static_cast<sentry_node_type*>(head));
        allocator_traits<sentry_allocator_type>::deconstruct_and_deallocate(static_cast<sentry_node_type*>(tail));
    }

public:
    list() noexcept
        // size is stored in the 'head' node
        : head(allocator_traits<sentry_allocator_type>::allocate_and_construct(0))
        , tail(allocator_traits<sentry_allocator_type>::allocate_and_construct(0))
    {
        head->connect(static_cast<node_type*>(tail));
        tail->connect(static_cast<node_type*>(head));
    }

    list(const list& v)
        : list()
    {
        for (const auto& item : v)
            push_back(item);
    }

    list(list&& v)
        : head(v.head)
        , tail(v.tail)
    {
        v.head = nullptr;
        v.tail = nullptr;
    }

    list& operator=(const list& v)
    {
        clear();
        for (const auto& item : v)
            push_back(item);
        return *this;
    }

    list& operator=(list&& v)
    {
        destroy();

        head = v.head;
        tail = v.tail;
        v.head = nullptr;
        v.tail = nullptr;

        return *this;
    }

    ~list() noexcept
    {
        destroy();
    }

    iterator_type find(const value_type& v) noexcept
    {
        for (iterator_type iter = begin(); iter != end(); ++iter)
            if (*iter == v)
                return iter;
    }

    // erase the node which iter points to
    iterator_type erase(const iterator_type& iter) noexcept
    {
        node_base_type* current_node = iter._node();
        iterator_type ret(current_node->next);
        current_node->prev->connect(current_node->next);
        allocator_traits<allocator_type>::deconstruct_and_deallocate(static_cast<node_type*>(current_node));
        --_size();
        return ret;
    }

    void clear(void)
    {
        for (auto iter = begin(); iter != end();)
            iter = erase(iter);
    }

    // insert the value v in front of the given iterator
    iterator_type insert(const iterator_type& iter, const value_type& v) noexcept
    {
        node_type* new_node = allocator_traits<allocator_type>::allocate_and_construct(v);
        iterator_type ret(new_node);
        iter._node()->prev->connect(new_node);
        new_node->connect(static_cast<node_type*>(iter._node()));

        ++_size();
        return ret;
    }

    // insert the value v in front of the given iterator
    iterator_type insert(const iterator_type& iter, value_type&& v) noexcept
    {
        node_type* new_node = allocator_traits<allocator_type>::allocate_and_construct(move(v));
        iterator_type ret(new_node);
        iter._node()->prev->connect(new_node);
        new_node->connect(static_cast<node_type*>(iter._node()));

        ++_size();
        return ret;
    }

    void push_back(const value_type& v) noexcept
    {
        insert(end(), v);
    }

    void push_back(value_type&& v) noexcept
    {
        insert(end(), move(v));
    }

    template <typename... Args>
    iterator_type emplace_back(Args&&... args)
    {
        return insert(end(), value_type(forward<Args>(args)...));
    }

    void push_front(const value_type& v) noexcept
    {
        insert(begin(), v);
    }

    void push_front(value_type&& v) noexcept
    {
        insert(begin(), move(v));
    }

    template <typename... Args>
    iterator_type emplace_front(Args&&... args)
    {
        return insert(begin(), value_type(forward<Args>(args)...));
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
        return iterator_type(static_cast<node_type*>(tail));
    }

    const_iterator_type begin() const noexcept
    {
        return const_iterator_type(head->next);
    }

    const_iterator_type end() const noexcept
    {
        return const_iterator_type(static_cast<node_type*>(tail));
    }

    const_iterator_type cbegin() const noexcept
    {
        return begin();
    }

    const_iterator_type cend() const noexcept
    {
        return end();
    }

    bool empty(void) const noexcept
    {
        return size() == 0;
    }

    // TODO
    // iterator_type r_start() noexcept;
    // iterator_type r_end() noexcept;

    // iterator_type cr_start() noexcept;
    // iterator_type cr_end() noexcept;
};

} // namespace types
