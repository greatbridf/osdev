#pragma once

#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/list.hpp>
#include <types/stdint.h>
#include <types/string.hpp>
#include <types/types.h>
#include <types/vector.hpp>

namespace types {

// taken from linux
constexpr uint32_t GOLDEN_RATIO_32 = 0x61C88647;
// constexpr uint64_t GOLDEN_RATIO_64 = 0x61C8864680B583EBull;

static constexpr uint32_t _hash32(uint32_t val)
{
    return val * GOLDEN_RATIO_32;
}

static constexpr uint32_t hash32(uint32_t val, uint32_t bits)
{
    // higher bits are more random
    return _hash32(val) >> (32 - bits);
}

template <typename T>
struct linux_hasher {
    static constexpr uint32_t hash(const T& val, uint32_t bits)
    {
        return hash32(val, bits);
    }
};

template <typename T>
struct linux_hasher<T*> {
    static constexpr uint32_t hash(T* val, uint32_t bits)
    {
        return hash32(reinterpret_cast<uint32_t>(val), bits);
    }
};

template <typename T>
struct string_hasher {
    static constexpr uint32_t hash(T, uint32_t)
    {
        static_assert(types::template_false_type<T>::value, "string hasher does not support this type");
        return (uint32_t)0;
    }
};
template <>
struct string_hasher<const char*> {
    static constexpr uint32_t hash(const char* str, uint32_t bits)
    {
        constexpr uint32_t seed = 131;
        uint32_t hash = 0;

        while (*str)
            hash = hash * seed + (*str++);

        return hash32(hash, bits);
    }
};
template <template <typename> class Allocator>
struct string_hasher<const types::string<Allocator>&> {
    static inline constexpr uint32_t hash(const types::string<Allocator>& str, uint32_t bits)
    {
        return string_hasher<const char*>::hash(str.c_str(), bits);
    }
};
template <template <typename> class Allocator>
struct string_hasher<types::string<Allocator>&&> {
    static inline constexpr uint32_t hash(types::string<Allocator>&& str, uint32_t bits)
    {
        return string_hasher<const char*>::hash(str.c_str(), bits);
    }
};

template <class Hasher, typename Value>
struct hasher_traits {
    using hash_t = size_t;
    using length_t = size_t;
    static constexpr hash_t hash(Value val, length_t bits)
    {
        return Hasher::hash(val, bits);
    }
};

template <typename Key, typename Value, typename Hasher, template <typename _T> class Allocator = types::kernel_allocator>
class hash_map {
public:
    struct pair;
    template <typename Pointer>
    class iterator;

    using key_type = Key;
    using value_type = Value;
    using pair_type = pair;
    using size_type = size_t;
    using difference_type = ssize_t;
    using iterator_type = iterator<pair_type*>;
    using const_iterator_type = iterator<const pair_type*>;

    using bucket_type = list<pair, Allocator>;
    using bucket_array_type = vector<bucket_type, Allocator>;

    static constexpr size_type INITIAL_BUCKETS_ALLOCATED = 64;

public:
    struct pair {
        const key_type key;
        value_type value;

        pair(void) = delete;
        pair(const key_type _key, value_type _val)
            : key(_key)
            , value(_val)
        {
        }
        bool operator==(const pair& p)
        {
            return key == p.key;
        }
    };

    template <typename Pointer>
    class iterator {
    public:
        using _Value = typename traits::remove_pointer<Pointer>::type;
        using Reference = typename traits::add_reference<_Value>::type;

    public:
        iterator(const iterator& iter) noexcept
            : p(iter.p)
        {
        }

        iterator(iterator&& iter) noexcept
            : p(iter.p)
        {
            iter.p = nullptr;
        }

        iterator& operator=(const iterator& iter)
        {
            p = iter.p;
            return *this;
        }

        explicit iterator(Pointer p) noexcept
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

        bool operator!()
        {
            return !p;
        }

        Reference operator*() const noexcept
        {
            return *p;
        }
        Pointer operator->() const noexcept
        {
            return p;
        }

    protected:
        Pointer p;
    };

private:
    bucket_array_type buckets;

protected:
    constexpr uint32_t hash_length(void) const
    {
        switch (buckets.capacity()) {
        case 32:
            return 5;
        case 64:
            return 6;
        case 128:
            return 7;
        case 256:
            return 8;
        // TODO
        default:
            return 9;
        }
    }

public:
    explicit hash_map(void)
        : buckets(INITIAL_BUCKETS_ALLOCATED)
    {
        for (size_type i = 0; i < INITIAL_BUCKETS_ALLOCATED; ++i)
            buckets.emplace_back();
    }

    hash_map(const hash_map& v)
        : buckets(v.buckets)
    {
    }

    hash_map(hash_map&& v)
        : buckets(move(v.buckets))
    {
    }

    ~hash_map()
    {
        buckets.clear();
    }

    void insert(const pair& p)
    {
        auto hash_value = hasher_traits<Hasher, key_type>::hash(p.key, hash_length());
        buckets.at(hash_value).push_back(p);
    }
    void insert(pair&& p)
    {
        auto hash_value = hasher_traits<Hasher, key_type>::hash(p.key, hash_length());
        buckets.at(hash_value).push_back(move(p));
    }
    void insert(const key_type& key, const value_type& val)
    {
        insert(pair { key, val });
    }

    void remove(const key_type& key)
    {
        auto hash_value = hasher_traits<Hasher, key_type>::hash(key, hash_length());
        auto& bucket = buckets.at(hash_value);
        for (auto iter = bucket.begin(); iter != bucket.end(); ++iter) {
            if (iter->key == key) {
                bucket.erase(iter);
                return;
            }
        }
    }

    iterator_type find(const key_type& key)
    {
        auto hash_value = hasher_traits<Hasher, key_type>::hash(key, hash_length());
        auto& bucket = buckets.at(hash_value);
        for (auto& item : bucket) {
            if (key == item.key)
                return iterator_type(&item);
        }
        return iterator_type(nullptr);
    }

    const_iterator_type find(const key_type& key) const
    {
        auto hash_value = hasher_traits<Hasher, key_type>::hash(key, hash_length());
        const auto& bucket = buckets.at(hash_value);
        for (const auto& item : bucket) {
            if (key == item.key)
                return const_iterator_type(&(*item));
        }
        return const_iterator_type(nullptr);
    }

    void clear(void)
    {
        for (size_t i = 0; i < buckets.size(); ++i)
            buckets.at(i).clear();
    }
};

} // namespace types