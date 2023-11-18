#pragma once
#include <utility>
#include <type_traits>

namespace types {

template <typename Key, typename Value>
struct pair {
    using key_type = Key;
    using value_type = Value;

    key_type key;
    value_type value;

    constexpr pair(void) = delete;
    constexpr ~pair()
    {
    }

    template <typename _key_type, typename _value_type>
    constexpr pair(_key_type&& _key, _value_type&& _value)
        : key(std::forward<_key_type>(_key))
        , value(std::forward<_value_type>(_value))
    {
    }

    template <typename _key_type, typename _value_type>
    constexpr pair(const pair<_key_type, _value_type>& val)
        : key(val.key)
        , value(val.value)
    {
        static_assert(std::is_same_v<std::decay_t<_key_type>, std::decay_t<key_type>>);
        static_assert(std::is_same_v<std::decay_t<_value_type>, std::decay_t<value_type>>);
    }
    template <typename _key_type, typename _value_type>
    constexpr pair(pair<_key_type, _value_type>&& val)
        : key(std::move(val.key))
        , value(std::move(val.value))
    {
        static_assert(std::is_same_v<std::decay_t<_key_type>, std::decay_t<key_type>>);
        static_assert(std::is_same_v<std::decay_t<_value_type>, std::decay_t<value_type>>);
    }
    constexpr pair(const pair& val)
        : key(val.key)
        , value(val.value)
    {
    }
    constexpr pair(pair&& val)
        : key(std::move(val.key))
        , value(std::move(val.value))
    {
    }
    constexpr pair& operator=(const pair& val)
    {
        key = val.key;
        value = val.vaule;
    }
    constexpr pair& operator=(pair&& val)
    {
        key = std::move(val.key);
        value = std::move(val.value);
    }

    constexpr bool key_eq(const pair& p)
    {
        return key == p.key;
    }

    constexpr bool value_eq(const pair& p)
    {
        return value == p.value;
    }

    constexpr bool operator==(const pair& p)
    {
        return key_eq(p) && value_eq(p);
    }

    constexpr bool operator!=(const pair& p)
    {
        return !this->operator==(p);
    }
};

template <typename T1, typename T2>
constexpr pair<std::decay_t<T1>, std::decay_t<T2>>
make_pair(T1&& t1, T2&& t2)
{
    return pair<std::decay_t<T1>, std::decay_t<T2>> { std::forward<T1>(t1), std::forward<T2>(t2) };
}

} // namespace types
