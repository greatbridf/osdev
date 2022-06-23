#pragma once

#ifdef __cplusplus

namespace types::traits::inner {

template <typename Tp, typename>
struct remove_pointer {
    using type = Tp;
};

template <typename Tp, typename T>
struct remove_pointer<Tp, T*> {
    using type = T;
};

template <typename Tr, typename>
struct remove_reference {
    using type = Tr;
};

template <typename Tr, typename T>
struct remove_reference<Tr, T&> {
    using type = T;
};

} // namespace types::traits::inner

namespace types::traits {

template <typename Tp>
struct remove_pointer
    : inner::remove_pointer<Tp, Tp> {
};

template <typename Tr>
struct remove_reference
    : inner::remove_reference<Tr, Tr> {
};

template <typename T>
struct add_pointer {
    using type = T*;
};

template <typename T>
struct add_reference {
    using type = T&;
};

} // namespace types::traits

namespace types {
template <typename T>
T&& move(T& val)
{
    return static_cast<T&&>(val);
}
template <typename T>
T&& forward(typename traits::remove_reference<T>::type& val)
{
    return static_cast<T&&>(val);
}

} // namespace types

#endif
