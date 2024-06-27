#pragma once
#include <assert.h>
#include <bit>
#include <map>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

#include <stdint.h>
#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/types.h>

namespace types {

// taken from linux
constexpr uint32_t GOLDEN_RATIO_32 = 0x61C88647;
constexpr uint64_t GOLDEN_RATIO_64 = 0x61C8864680B583EBull;

using hash_t = std::size_t;

static inline constexpr hash_t _hash32(uint32_t val)
{
    return val * GOLDEN_RATIO_32;
}

static inline constexpr hash_t hash32(uint32_t val, std::size_t bits)
{
    // higher bits are more random
    return _hash32(val) >> (8 * sizeof(hash_t) - bits);
}

static inline constexpr hash_t _hash64(uint64_t val)
{
    return val * GOLDEN_RATIO_64;
}

static inline constexpr hash_t hash64(uint64_t val, std::size_t bits)
{
    // higher bits are more random
    return _hash64(val) >> (8 * sizeof(hash_t) - bits);
}

template <typename T>
constexpr bool is_c_string_v = std::is_same_v<std::decay_t<T>, char*>
    || std::is_same_v<std::decay_t<T>, const char*>;

template <typename T,
    std::enable_if_t<std::is_convertible_v<T, uint64_t>, bool> = true>
inline hash_t hash(T val, std::size_t bits)
{
    return hash64(static_cast<uint64_t>(val), bits);
}

template <typename T,
    std::enable_if_t<std::is_pointer_v<T> && !is_c_string_v<T>, bool> = true>
inline hash_t hash(T val, std::size_t bits)
{
    return hash(std::bit_cast<uintptr_t>(val), bits);
}

inline hash_t hash(const char* str, std::size_t bits)
{
        constexpr uint32_t seed = 131;
        uint32_t hash = 0;

        while (*str)
            hash = hash * seed + (*str++);

        return hash64(hash, bits);
};

template <template <typename, typename, typename> typename String,
    typename Char, typename Traits, typename Allocator,
    std::enable_if_t<
        std::is_same_v<
            std::decay_t<String<Char, Traits, Allocator>>,
            std::basic_string<Char, Traits, Allocator>
        >, bool
    > = true>
inline hash_t hash(const String<Char, Traits, Allocator>& str, std::size_t bits)
{
    return hash(str.c_str(), bits);
}

template <typename Key, typename Value,
    typename Allocator = std::allocator<std::pair<const Key, Value> >,
    std::enable_if_t<std::is_convertible_v<hash_t, decltype(
        hash(std::declval<Key>(), std::declval<std::size_t>())
    )>, bool> = true>
class hash_map {
public:
    template <bool Const>
    class iterator;

    using key_type = std::add_const_t<Key>;
    using value_type = Value;
    using size_type = size_t;
    using difference_type = ssize_t;
    using iterator_type = iterator<false>;
    using const_iterator_type = iterator<true>;

    using bucket_type = std::map<key_type,
          value_type,std::less<key_type>, Allocator>;

    using bucket_array_type = std::vector<bucket_type, typename
        std::allocator_traits<Allocator>:: template rebind_alloc<bucket_type>>;

    static constexpr size_type INITIAL_BUCKETS_ALLOCATED = 64;

public:
    template <bool Const>
    class iterator {
    public:
        using bucket_iterator = std::conditional_t<Const,
              typename bucket_type::const_iterator,
              typename bucket_type::iterator>;
        using _Value = typename bucket_iterator::value_type;
        using Pointer = typename bucket_iterator::pointer;
        using Reference = typename bucket_iterator::reference;
        using hash_map_pointer = std::conditional_t<Const,
              const hash_map*, hash_map*>;

        friend class hash_map;

    public:
        constexpr iterator(const iterator& iter) noexcept
            : n(iter.n), iter(iter.iter), hmap(iter.hmap)
        {
        }

        constexpr iterator(iterator&& iter) noexcept
            : n(std::exchange(iter.n, 0))
            , iter(std::move(iter.iter))
            , hmap(std::exchange(iter.hmap, nullptr))
        {
        }

        constexpr iterator& operator=(const iterator& iter)
        {
            n = iter.n;
            this->iter = iter.iter;
            hmap = iter.hmap;
            return *this;
        }

        explicit constexpr iterator(std::size_t n, bucket_iterator iter,
                hash_map_pointer hmap) noexcept
            : n(n), iter(iter), hmap(hmap)
        {
        }

        constexpr bool operator==(const iterator& iter) const noexcept
        {
            return (!*this && !iter) || (hmap == iter.hmap && n == iter.n && this->iter == iter.iter);
        }

        constexpr bool operator!=(const iterator& iter) const noexcept
        {
            return !(*this == iter);
        }

        constexpr iterator operator++()
        {
            assert((bool)*this);

            ++iter;
            while (iter == hmap->buckets[n].end()) {
                ++n;
                if (n < hmap->buckets.size())
                    iter = hmap->buckets[n].begin();
                else
                    break;
            }

            return *this;
        }

        constexpr iterator operator++(int)
        {
            iterator ret { *this };

            (void)this->operator++();

            return ret;
        }

        constexpr operator bool(void) const
        {
            return hmap && n < hmap->buckets.size() && !!iter;
        }

        constexpr Reference operator*(void) const noexcept
        {
            return *iter;
        }
        constexpr Pointer operator->(void) const noexcept
        {
            return &*iter;
        }

        constexpr operator const_iterator_type() const noexcept
        {
            return const_iterator_type(n, iter, hmap);
        }

    protected:
        std::size_t n;
        bucket_iterator iter;
        hash_map_pointer hmap;
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
        : buckets(INITIAL_BUCKETS_ALLOCATED) {}

    constexpr hash_map(const hash_map& v)
        : buckets(v.buckets) {}

    constexpr hash_map(hash_map&& v)
        : buckets(std::move(v.buckets)) {}

    constexpr ~hash_map()
    {
        buckets.clear();
    }

    template <typename... Args>
    constexpr void emplace(Args&&... args)
    {
        std::pair<Key, Value> to_insert{std::forward<Args>(args)...};

        auto hash_value = hash(to_insert.first, hash_length());
        buckets.at(hash_value).emplace(to_insert);
    }

    constexpr void remove(const_iterator_type iter)
    {
        auto& bucket = buckets[iter.n];
        bucket.erase(iter.iter);
    }

    constexpr void remove(iterator_type iter)
    {
        return remove((const_iterator_type)iter);
    }

    constexpr void remove(const key_type& key)
    {
        const_iterator_type iter = find(key);
        if (!iter)
            return;

        remove(iter);
    }

    constexpr iterator_type find(const key_type& key)
    {
        auto hash_value = hash(key, hash_length());
        auto& bucket = buckets.at(hash_value);
        for (auto iter = bucket.begin(); iter != bucket.end(); ++iter) {
            if (key == iter->first)
                return iterator_type(hash_value, iter, this);
        }
        return this->end();
    }

    constexpr const_iterator_type find(const key_type& key) const
    {
        auto hash_value = hash(key, hash_length());
        const auto& bucket = buckets.at(hash_value);
        for (auto iter = bucket.cbegin(); iter != bucket.cend(); ++iter) {
            if (key == iter->first)
                return const_iterator_type(hash_value, iter, this);
        }
        return this->cend();
    }

    constexpr void clear(void)
    {
        for (auto& bucket : buckets)
            bucket.clear();
    }

    constexpr const_iterator_type cend() const noexcept
    {
        return const_iterator_type(buckets.size(), buckets[0].end(), this);
    }

    constexpr const_iterator_type end() const noexcept
    {
        return cend();
    }

    constexpr iterator_type end() noexcept
    {
        return iterator_type(buckets.size(), buckets[0].end(), this);
    }

    constexpr const_iterator_type cbegin() const noexcept
    {
        for (std::size_t i = 0; i < buckets.size(); ++i) {
            if (buckets[i].empty())
                continue;
            return const_iterator_type(i, buckets[i].begin(), this);
        }
        return cend();
    }

    constexpr const_iterator_type begin() const noexcept
    {
        return cbegin();
    }

    constexpr iterator_type begin() noexcept
    {
        for (std::size_t i = 0; i < buckets.size(); ++i) {
            if (buckets[i].empty())
                continue;
            return iterator_type(i, buckets[i].begin(), this);
        }
        return end();
    }
};

} // namespace types
