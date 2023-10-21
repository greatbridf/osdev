#pragma once
#include <utility>

#include <types/cplusplus.hpp>

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
        static_assert(is_same<typename traits::decay<_key_type>::type, typename traits::decay<key_type>::type>::value);
        static_assert(is_same<typename traits::decay<_value_type>::type, typename traits::decay<value_type>::type>::value);
    }
    template <typename _key_type, typename _value_type>
    constexpr pair(pair<_key_type, _value_type>&& val)
        : key(std::move(val.key))
        , value(std::move(val.value))
    {
        static_assert(is_same<typename traits::decay<_key_type>::type, typename traits::decay<key_type>::type>::value);
        static_assert(is_same<typename traits::decay<_value_type>::type, typename traits::decay<value_type>::type>::value);
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
constexpr pair<typename traits::decay<T1>::type, typename traits::decay<T2>::type>
make_pair(T1&& t1, T2&& t2)
{
    return pair<typename traits::decay<T1>::type, typename traits::decay<T2>::type> { std::forward<T1>(t1), std::forward<T2>(t2) };
}

} // namespace types
