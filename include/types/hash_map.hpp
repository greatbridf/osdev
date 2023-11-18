#pragma once
#include <utility>
#include <type_traits>

#include <stdint.h>
#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/list.hpp>
#include <types/pair.hpp>
#include <types/string.hpp>
#include <types/types.h>
#include <types/vector.hpp>

namespace types {

// taken from linux
constexpr uint32_t GOLDEN_RATIO_32 = 0x61C88647;
// constexpr uint64_t GOLDEN_RATIO_64 = 0x61C8864680B583EBull;

using hash_t = size_t;

static inline constexpr hash_t _hash32(uint32_t val)
{
    return val * GOLDEN_RATIO_32;
}

static inline constexpr hash_t hash32(uint32_t val, uint32_t bits)
{
    // higher bits are more random
    return _hash32(val) >> (32 - bits);
}

template <typename T, typename = void>
struct linux_hasher {};

template <typename T>
inline constexpr bool is_c_string_v = std::is_same_v<std::decay_t<T>, char*>
    || std::is_same_v<std::decay_t<T>, const char*>;

template <typename T>
struct linux_hasher<T, std::enable_if_t<std::is_convertible_v<T, uint32_t>>> {
    static inline constexpr hash_t hash(T val, uint32_t bits)
    {
        return hash32(static_cast<uint32_t>(val), bits);
    }
};
template <typename T>
struct linux_hasher<T,
    std::enable_if_t<std::is_pointer_v<T> && !is_c_string_v<T>>> {
    static inline constexpr hash_t hash(T val, uint32_t bits)
    {
        return hash32(reinterpret_cast<uint32_t>(val), bits);
    }
};

template <typename T>
struct linux_hasher<T, std::enable_if_t<is_c_string_v<T>>> {
    static inline constexpr hash_t hash(const char* str, uint32_t bits)
    {
        constexpr uint32_t seed = 131;
        uint32_t hash = 0;

        while (*str)
            hash = hash * seed + (*str++);

        return hash32(hash, bits);
    }
};
template <
    template <template <typename> class> class String,
    template <typename> class Allocator>
struct linux_hasher<String<Allocator>,
    std::enable_if_t<
        std::is_same_v<
            std::decay_t<String<Allocator>>, types::string<Allocator>
        >
    >
> {
    static inline constexpr hash_t hash(types::string<Allocator>&& str, uint32_t bits)
    {
        return linux_hasher<const char*>::hash(str.c_str(), bits);
    }
    static inline constexpr hash_t hash(const types::string<Allocator>& str, uint32_t bits)
    {
        return linux_hasher<const char*>::hash(str.c_str(), bits);
    }
};

template <typename Key, typename Value,
    template <typename _Key, typename...> class Hasher = types::linux_hasher,
    template <typename _T> class Allocator = types::kernel_allocator,
    std::enable_if_t<std::is_convertible_v<hash_t, decltype(
        Hasher<Key>::hash(std::declval<Key>(), std::declval<uint32_t>())
    )>, bool> = true>
class hash_map {
public:
    template <typename Pointer>
    class iterator;

    using key_type = std::add_const_t<Key>;
    using value_type = Value;
    using pair_type = pair<key_type, value_type>;
    using size_type = size_t;
    using difference_type = ssize_t;
    using iterator_type = iterator<pair_type*>;
    using const_iterator_type = iterator<const pair_type*>;

    using bucket_type = list<pair_type, Allocator>;
    using bucket_array_type = vector<bucket_type, Allocator>;

    using hasher_type = Hasher<Key>;

    static constexpr size_type INITIAL_BUCKETS_ALLOCATED = 64;

public:
    template <typename Pointer>
    class iterator {
    public:
        using _Value = std::remove_pointer_t<Pointer>;
        using Reference = std::add_lvalue_reference_t<_Value>;

        friend class hash_map;

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

        constexpr operator bool(void)
        {
            return p != nullptr;
        }

        constexpr Reference operator*(void) const noexcept
        {
            return *p;
        }
        constexpr Pointer operator->(void) const noexcept
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
    explicit constexpr hash_map(void)
        : buckets(INITIAL_BUCKETS_ALLOCATED)
    {
        for (size_type i = 0; i < INITIAL_BUCKETS_ALLOCATED; ++i)
            buckets.emplace_back();
    }

    constexpr hash_map(const hash_map& v)
        : buckets(v.buckets)
    {
    }

    constexpr hash_map(hash_map&& v)
        : buckets(std::move(v.buckets))
    {
    }

    constexpr ~hash_map()
    {
        buckets.clear();
    }

    constexpr void emplace(pair_type&& p)
    {
        auto hash_value = hasher_type::hash(p.key, hash_length());
        buckets.at(hash_value).push_back(std::move(p));
    }

    template <typename _key_type, typename _value_type>
    constexpr void emplace(_key_type&& key, _value_type&& value)
    {
        emplace(make_pair(std::forward<_key_type>(key), std::forward<_value_type>(value)));
    }

    constexpr void remove(const key_type& key)
    {
        auto hash_value = hasher_type::hash(key, hash_length());
        auto& bucket = buckets.at(hash_value);
        for (auto iter = bucket.begin(); iter != bucket.end(); ++iter) {
            if (iter->key == key) {
                bucket.erase(iter);
                return;
            }
        }
    }

    constexpr void remove(iterator_type iter)
    {
        remove(iter->key);
        iter.p = nullptr;
    }
    constexpr void remove(const_iterator_type iter)
    {
        remove(iter->key);
        iter.p = nullptr;
    }

    constexpr iterator_type find(const key_type& key)
    {
        auto hash_value = hasher_type::hash(key, hash_length());
        auto& bucket = buckets.at(hash_value);
        for (auto& item : bucket) {
            if (key == item.key)
                return iterator_type(&item);
        }
        return iterator_type(nullptr);
    }

    constexpr const_iterator_type find(const key_type& key) const
    {
        auto hash_value = hasher_type::hash(key, hash_length());
        const auto& bucket = buckets.at(hash_value);
        for (auto iter = bucket.cbegin(); iter != bucket.cend(); ++iter) {
            if (key == iter->key)
                return const_iterator_type(&iter);
        }
        return const_iterator_type(nullptr);
    }

    constexpr void clear(void)
    {
        for (size_t i = 0; i < buckets.size(); ++i)
            buckets.at(i).clear();
    }
};

} // namespace types
