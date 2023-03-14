#pragma once

#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/pair.hpp>
#include <types/types.h>

namespace types {

template <typename Key, typename Value, template <typename _T> class _Allocator = kernel_allocator>
class map {
public:
    using key_type = typename traits::add_const<Key>::type;
    using value_type = Value;
    using pair_type = pair<key_type, value_type>;

    struct node {
        node* parent = nullptr;

        node* left = nullptr;
        node* right = nullptr;

        enum class node_color {
            RED,
            BLACK,
        } color
            = node_color::RED;

        pair_type v;

        constexpr node(pair_type&& pair)
            : v(move(pair))
        {
        }
        constexpr node(const pair_type& pair)
            : v(pair)
        {
        }

        constexpr node* grandparent(void) const
        {
            return this->parent->parent;
        }

        constexpr node* uncle(void) const
        {
            node* pp = this->grandparent();
            return (this->parent == pp->left) ? pp->right : pp->left;
        }

        constexpr node* leftmost(void)
        {
            node* nd = this;
            while (nd->left)
                nd = nd->left;
            return nd;
        }

        constexpr const node* leftmost(void) const
        {
            const node* nd = this;
            while (nd->left)
                nd = nd->left;
            return nd;
        }

        constexpr node* rightmost(void)
        {
            node* nd = this;
            while (nd->right)
                nd = nd->right;
            return nd;
        }

        constexpr const node* rightmost(void) const
        {
            const node* nd = this;
            while (nd->right)
                nd = nd->right;
            return nd;
        }

        constexpr node* next(void)
        {
            if (this->right) {
                return this->right->leftmost();
            } else {
                if (this->is_root()) {
                    return nullptr;
                } else if (this->is_left_child()) {
                    return this->parent;
                } else {
                    node* ret = this;
                    do {
                        ret = ret->parent;
                    } while (!ret->is_root() && !ret->is_left_child());
                    return ret->parent;
                }
            }
        }

        constexpr const node* next(void) const
        {
            if (this->right) {
                return this->right->leftmost();
            } else {
                if (this->is_root()) {
                    return nullptr;
                } else if (this->is_left_child()) {
                    return this->parent;
                } else {
                    const node* ret = this;
                    do {
                        ret = ret->parent;
                    } while (!ret->is_root() && !ret->is_left_child());
                    return ret->parent;
                }
            }
        }

        constexpr node* prev(void)
        {
            if (this->left) {
                return this->left->rightmost();
            } else {
                if (this->is_root()) {
                    return nullptr;
                } else if (this->is_right_child()) {
                    return this->parent;
                } else {
                    node* ret = this;
                    do {
                        ret = ret->parent;
                    } while (!ret->is_root() && !ret->is_right_child());
                    return ret->parent;
                }
            }
        }

        static constexpr bool is_red(node* nd)
        {
            return nd && nd->color == node_color::RED;
        }
        static constexpr bool is_black(node* nd)
        {
            return !node::is_red(nd);
        }

        constexpr const node* prev(void) const
        {
            if (this->left) {
                return this->left->rightmost();
            } else {
                if (this->is_root()) {
                    return nullptr;
                } else if (this->is_right_child()) {
                    return this->parent;
                } else {
                    const node* ret = this;
                    do {
                        ret = ret->parent;
                    } while (!ret->is_root() && !ret->is_right_child());
                    return ret->parent;
                }
            }
        }

        constexpr bool is_root(void) const
        {
            return this->parent == nullptr;
        }

        constexpr bool is_full(void) const
        {
            return this->left && this->right;
        }

        constexpr bool has_child(void) const
        {
            return this->left || this->right;
        }

        constexpr bool is_leaf(void) const
        {
            return !this->has_child();
        }

        constexpr bool is_left_child(void) const
        {
            return this == this->parent->left;
        }

        constexpr bool is_right_child(void) const
        {
            return this == this->parent->right;
        }

        constexpr void tored(void)
        {
            this->color = node_color::RED;
        }
        constexpr void toblack(void)
        {
            this->color = node_color::BLACK;
        }

        static constexpr void swap(node* first, node* second)
        {
            if (node::is_red(first)) {
                first->color = second->color;
                second->color = node_color::RED;
            } else {
                first->color = second->color;
                second->color = node_color::BLACK;
            }

            if (first->parent == second) {
                node* tmp = first;
                first = second;
                second = tmp;
            }

            node* p = first->parent;
            node* cl = first->left;
            node* cr = first->right;

            bool is_first_left = first->is_left_child();
            bool is_second_left = second->is_left_child();

            if (!first->is_root()) {
                if (is_first_left)
                    p->left = second;
                else
                    p->right = second;
            }

            first->left = second->left;
            first->right = second->right;
            if (first->left)
                first->left->parent = first;
            if (first->right)
                first->right->parent = first;

            if (second->parent == first) {
                first->parent = second;
                second->parent = p;

                if (is_second_left) {
                    if (cr)
                        cr->parent = second;
                    second->left = first;
                    second->right = cr;
                } else {
                    if (cl)
                        cl->parent = second;
                    second->right = first;
                    second->left = cl;
                }
            } else {
                first->parent = second->parent;

                if (cl)
                    cl->parent = second;
                if (cr)
                    cr->parent = second;
                second->left = cl;
                second->right = cr;

                if (!second->is_root()) {
                    if (is_second_left)
                        second->parent->left = first;
                    else
                        second->parent->right = first;
                }

                second->parent = p;
            }
        }
    };

    using allocator_type = _Allocator<node>;

    template <bool Const>
    class iterator {
    private:
        static constexpr bool _is_const_iterator = Const;

    public:
        using node_pointer_type = typename traits::condition<_is_const_iterator, const node*, node*>::type;
        using value_type = typename traits::condition<_is_const_iterator, const pair_type, pair_type>::type;
        using pointer_type = typename traits::add_pointer<value_type>::type;
        using reference_type = typename traits::add_reference<value_type>::type;

        friend class map;

    private:
        node_pointer_type p;

    public:
        explicit constexpr iterator(node_pointer_type ptr)
            : p { ptr }
        {
        }

        constexpr iterator(const iterator& iter)
            : p { iter.p }
        {
        }

        constexpr iterator(iterator&& iter)
            : p { iter.p }
        {
            iter.p = nullptr;
        }

        constexpr ~iterator()
        {
#ifndef NDEBUG
            p = nullptr;
#endif
        }

        constexpr iterator& operator=(const iterator& iter)
        {
            p = iter.p;
        }

        constexpr iterator& operator=(iterator&& iter)
        {
            p = iter.p;
            iter.p = nullptr;
        }

        constexpr bool operator==(const iterator& iter) const
        {
            return p == iter.p;
        }

        constexpr bool operator!=(const iterator& iter) const
        {
            return !this->operator==(iter);
        }

        constexpr reference_type operator*(void) const
        {
            return p->v;
        }

        constexpr pointer_type operator&(void) const
        {
            return &p->v;
        }

        constexpr pointer_type operator->(void) const
        {
            return this->operator&();
        }

        constexpr iterator& operator++(void)
        {
            p = p->next();
            return *this;
        }

        constexpr iterator operator++(int)
        {
            iterator ret(p);

            (void)this->operator++();

            return ret;
        }

        constexpr iterator& operator--(void)
        {
            p = p->prev();
            return *this;
        }

        constexpr iterator operator--(int)
        {
            iterator ret(p);

            (void)this->operator--();

            return ret;
        }

        constexpr operator bool(void)
        {
            return p;
        }
    };

    using iterator_type = iterator<false>;
    using const_iterator_type = iterator<true>;

private:
    node* root = nullptr;

private:
    static constexpr node* newnode(node* parent, const pair_type& val)
    {
        auto* ptr = allocator_traits<allocator_type>::allocate_and_construct(val);
        ptr->parent = parent;
        return ptr;
    }
    static constexpr node* newnode(node* parent, pair_type&& val)
    {
        auto* ptr = allocator_traits<allocator_type>::allocate_and_construct(move(val));
        ptr->parent = parent;
        return ptr;
    }
    static constexpr void delnode(node* nd)
    {
        allocator_traits<allocator_type>::deconstruct_and_deallocate(nd);
    }

    constexpr void rotateleft(node* rt)
    {
        node* nrt = rt->right;

        if (!rt->is_root()) {
            if (rt->is_left_child()) {
                rt->parent->left = nrt;
            } else {
                rt->parent->right = nrt;
            }
        } else {
            this->root = nrt;
        }

        nrt->parent = rt->parent;
        rt->parent = nrt;

        rt->right = nrt->left;
        nrt->left = rt;
    }

    constexpr void rotateright(node* rt)
    {
        node* nrt = rt->left;

        if (!rt->is_root()) {
            if (rt->is_left_child()) {
                rt->parent->left = nrt;
            } else {
                rt->parent->right = nrt;
            }
        } else {
            this->root = nrt;
        }

        nrt->parent = rt->parent;
        rt->parent = nrt;

        rt->left = nrt->right;
        nrt->right = rt;
    }

    constexpr void balance(node* nd)
    {
        if (nd->is_root()) {
            nd->toblack();
            return;
        }

        if (node::is_black(nd->parent))
            return;

        node* p = nd->parent;
        node* pp = nd->grandparent();
        node* uncle = nd->uncle();

        if (node::is_red(uncle)) {
            p->toblack();
            uncle->toblack();
            pp->tored();
            this->balance(pp);
            return;
        }

        if (p->is_left_child()) {
            if (nd->is_left_child()) {
                p->toblack();
                pp->tored();
                this->rotateright(pp);
            } else {
                this->rotateleft(p);
                this->balance(p);
            }
        } else {
            if (nd->is_right_child()) {
                p->toblack();
                pp->tored();
                this->rotateleft(pp);
            } else {
                this->rotateright(p);
                this->balance(p);
            }
        }
    }

    constexpr node* _find(const key_type& key) const
    {
        node* cur = root;

        for (; cur;) {
            if (cur->v.key == key)
                return cur;

            if (key < cur->v.key)
                cur = cur->left;
            else
                cur = cur->right;
        }

        return nullptr;
    }

    // this function DOES NOT dellocate the node
    // caller is responsible for freeing the memory
    // @param: nd is guaranteed to be a leaf node
    constexpr void _erase(node* nd)
    {
        if (nd->is_root())
            return;

        if (node::is_black(nd)) {
            node* p = nd->parent;
            node* s = nullptr;
            if (nd->is_left_child())
                s = p->right;
            else
                s = p->left;

            if (node::is_red(s)) {
                p->tored();
                s->toblack();
                if (nd->is_right_child()) {
                    this->rotateright(p);
                    s = p->left;
                } else {
                    this->rotateleft(p);
                    s = p->right;
                }
            }

            node* r = nullptr;
            if (node::is_red(s->left)) {
                r = s->left;
                if (s->is_left_child()) {
                    r->toblack();
                    s->color = p->color;
                    this->rotateright(p);
                    p->toblack();
                } else {
                    r->color = p->color;
                    this->rotateright(s);
                    this->rotateleft(p);
                    p->toblack();
                }
            } else if (node::is_red(s->right)) {
                r = s->right;
                if (s->is_left_child()) {
                    r->color = p->color;
                    this->rotateleft(s);
                    this->rotateright(p);
                    p->toblack();
                } else {
                    r->toblack();
                    s->color = p->color;
                    this->rotateleft(p);
                    p->toblack();
                }
            } else {
                s->tored();
                p->toblack();
                this->_erase(p);
            }
        }
    }

public:
    constexpr iterator_type end(void)
    {
        return iterator_type(nullptr);
    }
    constexpr const_iterator_type end(void) const
    {
        return const_iterator_type(nullptr);
    }
    constexpr const_iterator_type cend(void) const
    {
        return const_iterator_type(nullptr);
    }

    constexpr iterator_type begin(void)
    {
        return root ? iterator_type(root->leftmost()) : end();
    }
    constexpr const_iterator_type begin(void) const
    {
        return root ? const_iterator_type(root->leftmost()) : end();
    }
    constexpr const_iterator_type cbegin(void) const
    {
        return root ? const_iterator_type(root->leftmost()) : end();
    }

    constexpr iterator_type find(const key_type& key)
    {
        return iterator_type(_find(key));
    }
    constexpr const_iterator_type find(const key_type& key) const
    {
        return const_iterator_type(_find(key));
    }

    constexpr iterator_type insert(pair_type&& val)
    {
        node* cur = root;

        while (likely(cur)) {
            if (val.key < cur->v.key) {
                if (!cur->left) {
                    node* nd = newnode(cur, move(val));
                    cur->left = nd;
                    this->balance(nd);
                    return iterator_type(nd);
                } else {
                    cur = cur->left;
                }
            } else {
                if (!cur->right) {
                    node* nd = newnode(cur, move(val));
                    cur->right = nd;
                    this->balance(nd);
                    return iterator_type(nd);
                } else {
                    cur = cur->right;
                }
            }
        }

        root = newnode(nullptr, move(val));
        root->toblack();
        return iterator_type(root);
    }

    constexpr iterator_type erase(const iterator_type& iter)
    {
        node* nd = iter.p;
        if (!nd)
            return end();

        if (nd->is_root() && nd->is_leaf()) {
            delnode(nd);
            root = nullptr;
            return end();
        }

        node* next = nd->next();

        while (!nd->is_leaf()) {
            node* alt = nd->right ? nd->right->leftmost() : nd->left;
            if (nd->is_root()) {
                this->root = alt;
            }
            node::swap(nd, alt);
        }

        this->_erase(nd);

        if (nd->is_left_child())
            nd->parent->left = nullptr;
        else
            nd->parent->right = nullptr;

        delnode(nd);

        return iterator_type(next);
    }

    constexpr void remove(const key_type& key)
    {
        auto iter = this->find(key);
        if (iter != this->end())
            this->erase(iter);
    }

    // destroy a subtree without adjusting nodes to maintain binary tree properties
    constexpr void destroy(node* nd)
    {
        if (nd) {
            this->destroy(nd->left);
            this->destroy(nd->right);
            delnode(nd);
        }
    }

    explicit constexpr map(void)
    {
    }
    constexpr map(const map& val)
    {
        for (const auto& item : val)
            this->insert(item);
    }
    constexpr map(map&& val)
        : root(val.root)
    {
        val.root = nullptr;
    }
    constexpr map& operator=(const map& val)
    {
        this->destroy(root);
        for (const auto& item : val)
            this->insert(item);
    }
    constexpr map& operator=(map&& val)
    {
        this->destroy(root);
        root = val.root;
        val.root = nullptr;
    }
    constexpr ~map()
    {
        this->destroy(root);
    }
};

} // namespace types
